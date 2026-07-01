
/*
* TotalCount
*
* Fixed field order; any layout change requires full chain replay.
*/
combi_struct! { TotalCount,
	// diamond mint
	minted_diamond                : DiamondNumber
	hacd_bid_burn_238             : Uint12 // HAC unit238
	// channel (current)
	opening_channel               : Uint8
	channel_deposit_238           : Uint12 // HAC unit238
	channel_deposit_sat           : Uint8 // BTC sat
	channel_interest_238          : Uint8 // HAC unit238
	// asset
	created_asset                 : Uint8
	asset_issue_burn_238          : Uint12 // HAC unit238
	// fee burn (legacy tx / vm)
	tx_fee_burn90_238             : Uint12 // legacy type1/2 tx extra9 burn, HAC unit238
	ast_vm_gas_burn_238           : Uint12 // HAC unit238
	// diamond inscription
	diamond_engraved              : Uint8 // DiaInscPush action count
	diamond_insc_burn_238         : Uint12 // HAC unit238
	dia_insc_push                 : Uint8 // created entries
	dia_insc_clean                : Uint8 // clean action count
	dia_insc_edit                 : Uint8 // edit action count
	dia_insc_move                 : Uint8 // move action count
	dia_insc_drop                 : Uint8 // destroyed entries
	dia_insc_live_diamond         : Uint8 // HACD currently carrying inscriptions
	// channel (lifetime)
	channel_open_total            : Uint8
	channel_close_total           : Uint8
	channel_closed_hac_volume_238 : Uint12 // HAC principal settled from channels, unit238
	// contract
	contract_protocol_cost_burn_238 : Uint12 // HAC unit238
	contract_deploy_count         : Uint8
	contract_update_count         : Uint8
	contract_charge_bytes_total   : Uint12 // raw bytes
	// tx fee flow
	tx_fee_pay_total_238          : Uint12 // HAC unit238
	tx_fee_got_total_238          : Uint12 // HAC unit238
	// blackhole burn
	blackhole_hac_burn_238        : Uint12 // HAC unit238
	blackhole_sat_burn            : Uint8  // BTC sat
	blackhole_asset_burn_count    : Uint8  // burn events to ADDRESS_ZERO
	blackhole_hacd_burn_count     : Uint8  // HACD burn events to ADDRESS_ZERO
}

#[cfg(test)]
mod total_count_tests {
	use super::*;

	#[test]
	fn total_count_serialize_parse_roundtrip() {
		let mut tc = TotalCount::default();
		tc.minted_diamond = DiamondNumber::from(3u32);
		tc.hacd_bid_burn_238 = Uint12::from(7u128);
		tc.channel_deposit_sat = Uint8::from(23u64);
		tc.diamond_engraved = Uint8::from(31u64);
		tc.dia_insc_push = Uint8::from(5u64);
		tc.dia_insc_drop = Uint8::from(2u64);
		tc.contract_deploy_count = Uint8::from(9u64);

		let bytes = tc.serialize();
		let got = TotalCount::build(&bytes).unwrap();
		assert_eq!(got, tc);
	}

	#[test]
	fn total_count_rejects_truncated_bytes() {
		let bytes = TotalCount::default().serialize();
		assert!(TotalCount::build(&bytes[..bytes.len().saturating_sub(1)]).is_err());
	}
}
