fn transaction_sign(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let unit = q_string(&req, "unit", "fin");
    let prikey = q_string(&req, "prikey", "");
    let pubkey = q_string(&req, "pubkey", "");
    let sigdts = q_string(&req, "sigdts", "");
    let signature = q_bool(&req, "signature", false);
    let description = q_bool(&req, "description", false);

    let lasthei = ctx.engine.latest_block().height().uint();
    let Ok(txdts) = body_data_may_hex(&req) else {
        return api_error("transaction body invalid");
    };
    let Ok((mut tx, _)) = protocol::transaction::transaction_create(&txdts) else {
        return api_error("transaction body invalid");
    };
    if let Some(resp) = reject_api_tx_non_canonical_dia_insc_push_wire(tx.as_read()) {
        return resp;
    }

    let (address, signobj) = if prikey.len() == 64 {
        let Ok(prik) = hex::decode(&prikey) else {
            return api_error("prikey format invalid");
        };
        let Ok(acc) = Account::create_by_secret_key_value(prik.try_into().unwrap()) else {
            return api_error("prikey data invalid");
        };
        let fres = tx.fill_sign(&acc);
        if let Err(e) = fres {
            return api_error(&format!("fill sign failed: {}", e));
        }
        (Address::from(*acc.address()), fres.unwrap())
    } else {
        if pubkey.len() != 33 * 2 || sigdts.len() != 64 * 2 {
            return api_error("pubkey or signature data invalid");
        }
        let Ok(pbk) = hex::decode(&pubkey) else {
            return api_error("pubkey format invalid");
        };
        let Ok(sig) = hex::decode(&sigdts) else {
            return api_error("sigdts format invalid");
        };
        let pbk: [u8; 33] = pbk.try_into().unwrap();
        let sig: [u8; 64] = sig.try_into().unwrap();
        let signobj = field::Sign {
            publickey: Fixed33::from(pbk),
            signature: Fixed64::from(sig),
        };
        if let Err(e) = tx.push_sign(signobj.clone()) {
            return api_error(&format!("fill sign failed: {}", e));
        }
        (Address::from(Account::get_address_by_public_key(pbk)), signobj)
    };

    let mut data = render_tx_info(
        tx.as_read(),
        None,
        lasthei,
        &unit,
        true,
        signature,
        false,
        description,
    );
    data.insert(
        "sign_data".to_owned(),
        json!({
            "address": address.to_readable(),
            "pubkey": signobj.publickey.to_hex(),
            "sigdts": signobj.signature.to_hex(),
        }),
    );
    api_data(data)
}

fn create_transaction_error_response(
    code: &str,
    message: &str,
    stage: &str,
    details: Vec<(&str, Value)>,
) -> ApiResponse {
    let mut data = serde_json::Map::new();
    data.insert("ret".to_owned(), json!(1));
    data.insert("err".to_owned(), json!(message));
    data.insert("error".to_owned(), json!(message));
    data.insert("code".to_owned(), json!(code));
    data.insert("message".to_owned(), json!(message));
    data.insert("stage".to_owned(), json!(stage));
    for (k, v) in details {
        data.insert(k.to_owned(), v);
    }
    ApiResponse::json(Value::Object(data).to_string())
}

fn transaction_build(_ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    transaction_build_inner(&req)
}

fn transaction_build_inner(req: &ApiRequest) -> ApiResponse {
    let unit = q_string(&req, "unit", "fin");
    let action = q_bool(&req, "action", false);
    let signature = q_bool(&req, "signature", false);
    let description = q_bool(&req, "description", false);

    let api_error_create_transaction = |code: &str, message: &str, stage: &str| {
        create_transaction_error_response(code, message, stage, vec![])
    };

    let Ok(txjsondts) = body_data_may_hex(&req) else {
        return api_error_create_transaction(
            "create_transaction_invalid_json_body",
            "transaction JSON body invalid",
            "parse_body",
        );
    };
    let Ok(jsonstr) = std::str::from_utf8(&txjsondts) else {
        return api_error_create_transaction(
            "create_transaction_invalid_json_body",
            "transaction JSON body invalid",
            "parse_body",
        );
    };
    let Ok(jsonv) = serde_json::from_str::<serde_json::Value>(jsonstr) else {
        return api_error_create_transaction(
            "create_transaction_invalid_json_body",
            "transaction JSON body invalid",
            "parse_body",
        );
    };

    let Some(main_addr) = jsonv["main_address"].as_str() else {
        return create_transaction_error_response(
            "create_transaction_invalid_main_address",
            "main_address format invalid",
            "parse_main_address",
            vec![("field", json!("main_address"))],
        );
    };
    let Ok(main_addr) = Address::from_readable(main_addr) else {
        return create_transaction_error_response(
            "create_transaction_invalid_main_address",
            "main_address format invalid",
            "parse_main_address",
            vec![("field", json!("main_address"))],
        );
    };
    let Some(fee) = jsonv["fee"].as_str() else {
        return create_transaction_error_response(
            "create_transaction_invalid_fee",
            "fee format invalid",
            "parse_fee",
            vec![("field", json!("fee"))],
        );
    };
    let Ok(fee) = Amount::from(fee) else {
        return create_transaction_error_response(
            "create_transaction_invalid_fee",
            "fee format invalid",
            "parse_fee",
            vec![("field", json!("fee"))],
        );
    };

    let tx_type = jsonv
        .get("tx_type")
        .or_else(|| jsonv.get("type"))
        .and_then(Value::as_u64)
        .unwrap_or(TransactionType2::TYPE as u64);
    let timestamp = jsonv["timestamp"].as_u64().unwrap_or_else(curtimes);
    let mut tx: Box<dyn Transaction> = match tx_type {
        v if v == TransactionType2::TYPE as u64 => {
            Box::new(TransactionType2::new_by(main_addr, fee, timestamp))
        }
        v if v == TransactionType3::TYPE as u64 => {
            let gas_max = jsonv["gas_max"].as_u64().unwrap_or(0);
            if gas_max > protocol::context::TX_GAS_BUDGET_CAP_BYTE as u64 {
                return create_transaction_error_response(
                    "create_transaction_invalid_gas_max",
                    "gas_max exceeds the current Type3 cap",
                    "parse_gas_max",
                    vec![
                        ("field", json!("gas_max")),
                        ("max", json!(protocol::context::TX_GAS_BUDGET_CAP_BYTE)),
                    ],
                );
            }
            let mut tx = TransactionType3::new_by(main_addr, fee, timestamp);
            tx.gas_max = Uint1::from(gas_max as u8);
            Box::new(tx)
        }
        _ => {
            return create_transaction_error_response(
                "create_transaction_invalid_type",
                "transaction type must be 2 or 3",
                "parse_type",
                vec![("field", json!("tx_type"))],
            );
        }
    };

    let Some(acts) = jsonv["actions"].as_array() else {
        return create_transaction_error_response(
            "create_transaction_invalid_actions",
            "actions array format invalid",
            "parse_actions",
            vec![("field", json!("actions"))],
        );
    };
    for (action_index, act) in acts.iter().enumerate() {
        let action_kind = act.get("kind").and_then(Value::as_u64);
        let a = match action_from_json(&act.to_string()) {
            Ok(v) => v,
            Err(e) => {
                let message = match action_kind {
                    Some(kind) => {
                        format!("transaction action[{action_index}] kind {kind} invalid: {e}")
                    }
                    None => format!("transaction action[{action_index}] invalid: {e}"),
                };
                let mut details = vec![
                    ("action_index", json!(action_index)),
                    ("cause", json!(e)),
                ];
                if let Some(kind) = action_kind {
                    details.push(("action_kind", json!(kind)));
                }
                return create_transaction_error_response(
                    "create_transaction_invalid_action",
                    &message,
                    "action_decode",
                    details,
                );
            }
        };
        if let Err(e) = tx.push_action(a) {
            let message = match action_kind {
                Some(kind) => {
                    format!("transaction action[{action_index}] kind {kind} rejected: {e}")
                }
                None => format!("transaction action[{action_index}] rejected: {e}"),
            };
            let mut details = vec![
                ("action_index", json!(action_index)),
                ("cause", json!(e)),
            ];
            if let Some(kind) = action_kind {
                details.push(("action_kind", json!(kind)));
            }
            return create_transaction_error_response(
                "create_transaction_action_rejected",
                &message,
                "action_push",
                details,
            );
        }
    }

    if reject_api_tx_non_canonical_dia_insc_push_wire(tx.as_read()).is_some() {
        return create_transaction_error_response(
            "create_transaction_non_canonical_protocol_cost",
            "DiaInscPush protocol_cost must use canonical amount encoding",
            "validate_protocol_cost_wire",
            vec![],
        );
    }

    api_data(render_tx_info(
        tx.as_read(),
        None,
        0,
        &unit,
        true,
        signature,
        action,
        description,
    ))
}

#[cfg(test)]
mod transaction_build_tests {
    use super::*;
    use std::collections::HashMap;

    fn install_protocol_setup() -> protocol::setup::TestSetupScopeGuard {
        let setup = protocol::setup::new_standard_protocol_setup(x16rs::block_hash);
        protocol::setup::install_test_scope(setup)
    }

    #[test]
    fn transaction_build_must_return_structured_action_decode_error() {
        let _guard = install_protocol_setup();
        let req = ApiRequest {
            body: json!({
                "main_address": "1AVRuFXNFi3rdMrPH4hdqSgFrEBnWisWaS",
                "fee": "1:244",
                "actions": [
                    {
                        "kind": 6,
                        "to": "1BcktgV7EjHmxEwQDFFhhztzNqZkd5gdm",
                        "diamonds": "WWWTTT"
                    }
                ]
            })
            .to_string()
            .into_bytes(),
            ..ApiRequest::default()
        };

        let res = transaction_build_inner(&req);
        let body: Value = serde_json::from_slice(&res.body).unwrap();

        assert_eq!(res.status, 200);
        assert_eq!(body["ret"].as_u64(), Some(1));
        assert_eq!(
            body["code"].as_str(),
            Some("create_transaction_invalid_action")
        );
        assert_eq!(body["stage"].as_str(), Some("action_decode"));
        assert_eq!(body["action_index"].as_u64(), Some(0));
        assert_eq!(body["action_kind"].as_u64(), Some(6));
        assert_eq!(body["err"], body["message"]);
        assert_eq!(body["error"], body["message"]);
        assert!(
            body["message"]
                .as_str()
                .unwrap_or_default()
                .contains("action[0]")
        );
        assert!(
            body["cause"]
                .as_str()
                .unwrap_or_default()
                .contains("missing required field(s): from")
        );
    }

    #[test]
    fn transaction_build_supports_type3_gas_max() {
        let _guard = install_protocol_setup();
        let req = ApiRequest {
            query: HashMap::from([("body".to_owned(), "true".to_owned())]),
            body: json!({
                "tx_type": 3,
                "main_address": "1AVRuFXNFi3rdMrPH4hdqSgFrEBnWisWaS",
                "fee": "1:244",
                "gas_max": 17,
                "actions": []
            })
            .to_string()
            .into_bytes(),
            ..ApiRequest::default()
        };

        let res = transaction_build_inner(&req);
        let body: Value = serde_json::from_slice(&res.body).unwrap();

        assert_eq!(body["ret"].as_u64(), Some(0));
        assert_eq!(body["type"].as_u64(), Some(3));
        assert_eq!(body["gas_max"].as_u64(), Some(17));
        let txhex = body["body"].as_str().unwrap();
        let txdts = hex::decode(txhex).unwrap();
        let (tx, _) = protocol::transaction::transaction_create(&txdts).unwrap();
        assert_eq!(tx.ty(), TransactionType3::TYPE);
        assert_eq!(tx.gas_max_byte(), Some(17));
    }

    #[test]
    fn transaction_build_rejects_type3_gas_max_above_cap() {
        let _guard = install_protocol_setup();
        let req = ApiRequest {
            body: json!({
                "tx_type": 3,
                "main_address": "1AVRuFXNFi3rdMrPH4hdqSgFrEBnWisWaS",
                "fee": "1:244",
                "gas_max": protocol::context::TX_GAS_BUDGET_CAP_BYTE as u64 + 1,
                "actions": []
            })
            .to_string()
            .into_bytes(),
            ..ApiRequest::default()
        };

        let res = transaction_build_inner(&req);
        let body: Value = serde_json::from_slice(&res.body).unwrap();

        assert_eq!(body["ret"].as_u64(), Some(1));
        assert_eq!(
            body["code"].as_str(),
            Some("create_transaction_invalid_gas_max")
        );
    }
}

fn transaction_check(_ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let unit = q_string(&req, "unit", "fin");
    let set_fee = q_string(&req, "set_fee", "");
    let sign_address = q_string(&req, "sign_address", "");
    let body = q_bool(&req, "body", false);
    let signature = q_bool(&req, "signature", false);
    let description = q_bool(&req, "description", false);

    let Ok(txdts) = body_data_may_hex(&req) else {
        return api_error("transaction body invalid");
    };
    let Ok((mut tx, _)) = protocol::transaction::transaction_create(&txdts) else {
        return api_error("transaction body invalid");
    };
    if let Some(resp) = reject_api_tx_non_canonical_dia_insc_push_wire(tx.as_read()) {
        return resp;
    }

    if !set_fee.is_empty() {
        let Ok(fee) = Amount::from(&set_fee) else {
            return api_error("fee format invalid");
        };
        tx.set_fee(fee);
    }

    let tx = tx.as_read();
    let mut data = render_tx_info(tx, None, 0, &unit, body, signature, true, description);
    if !sign_address.is_empty() {
        let Ok(addr) = Address::from_readable(&sign_address) else {
            return api_error("sign_address format invalid");
        };
        let sign_hash = if tx.main() == addr {
            tx.hash_with_fee()
        } else {
            tx.hash()
        };
        data.insert("sign_hash".to_owned(), json!(sign_hash.to_hex()));
    }

    api_data(data)
}

fn transaction_exist(ctx: &ApiExecCtx, req: ApiRequest) -> ApiResponse {
    let unit = q_string(&req, "unit", "fin");
    let hash = q_string(&req, "hash", "");
    let body = q_bool(&req, "body", false);
    let action = q_bool(&req, "action", false);
    let signature = q_bool(&req, "signature", false);
    let description = q_bool(&req, "description", false);
    let lasthei = ctx.engine.latest_block().height().uint();

    let Ok(hx) = hex::decode(&hash) else {
        return api_error("transaction hash format invalid");
    };
    if hx.len() != Hash::SIZE {
        return api_error("transaction hash format invalid");
    }
    let txhx = Hash::must(&hx);

    let txpool = ctx.hnoder.txpool();
    if let Some(txp) = txpool.find(&txhx) {
        let mut info = render_tx_info(
            txp.tx_read(),
            None,
            lasthei,
            &unit,
            body,
            signature,
            action,
            description,
        );
        info.insert("pending".to_owned(), json!(true));
        return api_data(info);
    }

    let state_ptr = read_mint_state(ctx);
    let state = CoreStateRead::wrap(state_ptr.as_ref().as_ref());
    let Some(txp) = state.tx_exist(&txhx) else {
        return api_error("transaction not found");
    };
    let Ok(blkpkg) = load_block_by_key(ctx, &txp.to_string()) else {
        return api_error("cannot find block by transaction ptr");
    };
    let blkobj = blkpkg.block();
    let blktrs = blkobj.transactions();

    let tx = {
        let txnum = blkobj.transaction_count().uint() as usize;
        let mut found = None;
        for it in blktrs[1..txnum].iter() {
            if txhx == it.hash() {
                found = Some(it.clone());
                break;
            }
        }
        found
    };
    let Some(tx) = tx else {
        return api_error("transaction not found in the block");
    };

    api_data(render_tx_info(
        tx.as_read(),
        Some(blkobj.as_read()),
        lasthei,
        &unit,
        body,
        signature,
        action,
        description,
    ))
}

fn render_tx_info(
    tx: &dyn TransactionRead,
    blblk: Option<&dyn BlockRead>,
    lasthei: u64,
    unit: &str,
    body: bool,
    signature: bool,
    action: bool,
    description: bool,
) -> serde_json::Map<String, Value> {
    let fee_str = tx.fee().to_unit_string(unit);
    let main_addr = tx.main().to_readable();
    let mut data = serde_json::Map::new();
    data.insert("hash".to_owned(), json!(tx.hash().to_hex()));
    data.insert("hash_with_fee".to_owned(), json!(tx.hash_with_fee().to_hex()));
    data.insert("type".to_owned(), json!(tx.ty()));
    data.insert("timestamp".to_owned(), json!(tx.timestamp().uint()));
    data.insert("fee".to_owned(), json!(fee_str));
    data.insert("fee_got".to_owned(), json!(tx.fee_got().to_unit_string(unit)));
    if let Some(gas_max) = tx.gas_max_byte() {
        data.insert("gas_max".to_owned(), json!(gas_max));
    }
    data.insert("main_address".to_owned(), json!(main_addr.clone()));
    data.insert("action".to_owned(), json!(tx.action_count()));

    if body {
        data.insert("body".to_owned(), json!(tx.serialize().to_hex()));
    }
    if signature {
        check_signature(&mut data, tx);
    }
    if description {
        data.insert(
            "description".to_owned(),
            json!(format!("Main account {} pay {} HAC tx fee", main_addr, fee_str)),
        );
    }
    if let Some(blkobj) = blblk {
        let txblkhei = blkobj.height().uint();
        data.insert(
            "block".to_owned(),
            json!({
                "height": txblkhei,
                "timestamp": blkobj.timestamp().uint(),
            }),
        );
        data.insert("confirm".to_owned(), json!(lasthei - txblkhei));
    }
    if action {
        let mut acts = Vec::with_capacity(tx.actions().len());
        for act in tx.actions() {
            acts.push(action_to_json_desc(tx, act, unit, description));
        }
        data.insert("actions".to_owned(), json!(acts));
    }
    data
}

fn check_signature(data: &mut serde_json::Map<String, Value>, tx: &dyn TransactionRead) {
    let Ok(sigstats) = check_tx_signature(tx) else {
        return;
    };
    let mut sigchs = vec![];
    for (adr, sg) in sigstats {
        sigchs.push(json!({
            "address": adr.to_readable(),
            "complete": sg,
        }));
    }
    data.insert("signatures".to_owned(), json!(sigchs));
}
