combi_struct! { CodeStuff,
    conf: Uint1
    data: BytesW2
}

pub const MAX_FUNC_PARAM_LEN: usize = 15;

impl CodeStuff {
    pub fn parse_conf(&self) -> VmrtRes<CodeConf> {
        CodeConf::parse(self.conf.uint())
    }
}

impl TryFrom<&CodeStuff> for CodePkg {
    type Error = ItrErr;

    fn try_from(src: &CodeStuff) -> Result<Self, Self::Error> {
        let conf = src.parse_conf()?.raw();
        Ok(Self {
            conf,
            data: src.data.to_vec(),
        })
    }
}

impl TryFrom<CodeStuff> for CodePkg {
    type Error = ItrErr;

    fn try_from(src: CodeStuff) -> Result<Self, Self::Error> {
        let conf = src.parse_conf()?.raw();
        Ok(Self {
            conf,
            data: src.data.into_vec(),
        })
    }
}

impl TryFrom<&CodePkg> for CodeStuff {
    type Error = ItrErr;

    fn try_from(src: &CodePkg) -> Result<Self, Self::Error> {
        let conf = CodeConf::parse(src.conf)?.raw();
        Ok(Self {
            conf: Uint1::from(conf),
            data: BytesW2::from(src.data.clone()).map_ire(ItrErrCode::CastParamFail)?,
        })
    }
}

impl TryFrom<CodePkg> for CodeStuff {
    type Error = ItrErr;

    fn try_from(src: CodePkg) -> Result<Self, Self::Error> {
        let conf = CodeConf::parse(src.conf)?.raw();
        Ok(Self {
            conf: Uint1::from(conf),
            data: BytesW2::from(src.data).map_ire(ItrErrCode::CastParamFail)?,
        })
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FuncArgvTypes {
    typnum: Uint1, // [ 4bit: output type, 4bit: inputs num]
    define: Vec<u8>,
}

impl FuncArgvTypes {
    fn def_size(&self) -> usize {
        let n = bit4r!(self.typnum.uint()) as usize;
        (n + 1) / 2
    }

    pub fn param_count(&self) -> usize {
        bit4r!(self.typnum.uint()) as usize
    }

    pub fn check_output(&self, v: &mut Value) -> VmrtErr {
        let Some(oty) = self.output_type().map_ire(CallArgvTypeFail)? else {
            return Ok(());
        };
        if let Err(e) = v.cast_param(oty) {
            return itr_err_fmt!(CallArgvTypeFail, "check output failed: {:?}", e);
        }
        Ok(())
    }

    pub fn check_params(&self, v: &mut Value) -> VmrtErr {
        let ec = CallArgvTypeFail;
        let types = self.param_types().map_ire(ec)?;
        match types.as_slice() {
            [] => Ok(()),
            [ty] => v.cast_param(*ty),
            tys => {
                // Typed multi-parameter ABI accepts only Tuple packing; plain Compo values,
                // including List, stay ordinary single values unless user code unpacks them.
                let Value::Tuple(tuple) = v else {
                    return itr_err_code!(CallArgvTypeFail);
                };
                let mut items = tuple.to_vec();
                if items.len() != tys.len() {
                    return itr_err_fmt!(
                        CallArgvTypeFail,
                        "param length invalid: expected {} but got {}",
                        tys.len(),
                        items.len()
                    );
                }
                for (item, ty) in items.iter_mut().zip(tys.iter().copied()) {
                    item.cast_param(ty)?;
                }

                *v = Value::Tuple(
                    TupleItem::new(items).map_err(|ItrErr(_, msg)| ItrErr::new(ec, &msg))?,
                );
                Ok(())
            }
        }
    }

    pub fn from_types(otp: Option<ValueTy>, tys: Vec<ValueTy>) -> Ret<Self> {
        let output_ty = match otp {
            Some(o) => {
                o.check_func_retv_type()?;
                (o as u8) << 4
            }
            _ => 0,
        };
        let n = tys.len();
        if n > MAX_FUNC_PARAM_LEN {
            return errf!("func types cannot exceed {}", MAX_FUNC_PARAM_LEN);
        }
        if 0 == n {
            return Ok(Self {
                typnum: Uint1::from(output_ty),
                define: vec![],
            });
        }
        let z = (n + 1) / 2;
        let mut dfs = vec![0u8; z];
        for i in 0..n {
            let ty = tys[i];
            ty.check_func_argv_type()?;
            let ty = ty as u8;
            let tn = maybe!(i % 2 == 0, ty << 4, ty);
            dfs[i / 2] = dfs[i / 2] | tn;
        }
        let typnum = output_ty | (n as u8);
        Ok(Self {
            typnum: Uint1::from(typnum),
            define: dfs,
        })
    }

    pub fn output_type(&self) -> Ret<Option<ValueTy>> {
        let tn = bit4l!(self.typnum.uint());
        let ty = ValueTy::build(tn)?;
        Ok(match ty {
            ValueTy::Nil => None,
            _ => {
                ty.check_func_retv_type()?;
                Some(ty)
            }
        })
    }

    pub fn param_types(&self) -> Ret<Vec<ValueTy>> {
        let n = self.param_count();
        if 0 == n {
            return Ok(vec![]);
        }
        let mut tys = vec![ValueTy::Nil; n];
        let z = (n + 1) / 2;
        if z > self.define.len() {
            return errf!("FuncArgvTypes to bytes conversion error");
        }
        for i in 0..n {
            let tn = self.define[i / 2];
            let t = maybe!(i % 2 == 0, bit4l!(tn), bit4r!(tn));
            let ty = ValueTy::build(t)?;
            ty.check_func_argv_type()?;
            tys[i] = ty;
        }
        Ok(tys)
    }
}

impl Parse for FuncArgvTypes {
    fn parse(&mut self, mut buf: &[u8]) -> Ret<usize> {
        self.typnum.parse(buf)?;
        buf = &buf[1..];
        let z = self.def_size();
        self.define = bufeat(buf, z)?;
        Ok(1 + z)
    }
}

impl Serialize for FuncArgvTypes {
    fn serialize_to(&self, out: &mut Vec<u8>) {
        let z = self.def_size();
        self.typnum.serialize_to(out);
        // `define` is always sized to `def_size()` by `parse` / `from_types`. Pad on the
        // fly in case a partial-constructed instance reaches `serialize` so we surface a
        // recoverable error instead of panicking.
        let take = self.define.len().min(z);
        out.extend_from_slice(&self.define[..take]);
        if z > take {
            out.extend(std::iter::repeat_n(0u8, z - take));
        }
    }
    fn size(&self) -> usize {
        1 + self.def_size()
    }
}

impl ToJSON for FuncArgvTypes {
    fn to_json_fmt(&self, _fmt: &JSONFormater) -> String {
        format!("\"{}\"", hex::encode(self.serialize()))
    }
}
impl FromJSON for FuncArgvTypes {
    fn from_json(&mut self, json: &str) -> Ret<()> {
        let data = hex::decode(json_unquote(json)).map_err(|_| format!("cannot decode hex"))?;
        self.parse(&data)?;
        Ok(())
    }
}

impl_field_only_new! {FuncArgvTypes}

#[cfg(test)]
mod code_stuff_tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn code_stuff_to_pkg_rejects_invalid_conf() {
        let mut code_stuff = CodeStuff::new();
        code_stuff.conf = Uint1::from(0b0000_0100);
        code_stuff.data = BytesW2::from(vec![Bytecode::END as u8]).unwrap();
        let err = CodePkg::try_from(&code_stuff).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CodeTypeError);
    }

    #[test]
    fn code_pkg_to_stuff_roundtrip() {
        let pkg = CodePkg {
            conf: CodeConf::from_type(CodeType::Bytecode).raw(),
            data: vec![Bytecode::END as u8],
        };
        let code_stuff = CodeStuff::try_from(pkg.clone()).unwrap();
        let back = CodePkg::try_from(code_stuff).unwrap();
        assert_eq!(back, pkg);
    }

    #[test]
    fn func_argv_types_even_params_uses_exact_nibble_bytes() {
        let src = FuncArgvTypes::from_types(None, vec![ValueTy::U8, ValueTy::U16]).unwrap();
        let raw = src.serialize();
        assert_eq!(raw.len(), 2);

        let mut parsed = FuncArgvTypes::new();
        let used = parsed.parse(&raw).unwrap();
        assert_eq!(used, raw.len());
        assert_eq!(parsed.param_count(), 2);
        assert_eq!(
            parsed.param_types().unwrap(),
            vec![ValueTy::U8, ValueTy::U16]
        );
    }

    #[test]
    fn func_argv_types_odd_params_still_roundtrip() {
        let src = FuncArgvTypes::from_types(
            Some(ValueTy::U64),
            vec![ValueTy::U8, ValueTy::U16, ValueTy::U32],
        )
        .unwrap();
        let raw = src.serialize();
        assert_eq!(raw.len(), 3);

        let mut parsed = FuncArgvTypes::new();
        let used = parsed.parse(&raw).unwrap();
        assert_eq!(used, raw.len());
        assert_eq!(parsed.output_type().unwrap(), Some(ValueTy::U64));
        assert_eq!(
            parsed.param_types().unwrap(),
            vec![ValueTy::U8, ValueTy::U16, ValueTy::U32]
        );
    }

    #[test]
    fn check_params_single_normalizes_uints_in_place() {
        let tys = FuncArgvTypes::from_types(None, vec![ValueTy::U16]).unwrap();
        let mut argv = Value::U8(7);
        tys.check_params(&mut argv).unwrap();
        assert_eq!(argv, Value::U16(7));
    }

    #[test]
    fn check_params_single_rejects_cross_family_implicit_casts() {
        let addr = field::Address::create_contract([1u8; 20]);
        let mut argv = Value::Bytes(addr.serialize());
        let err = FuncArgvTypes::from_types(None, vec![ValueTy::Address])
            .unwrap()
            .check_params(&mut argv)
            .unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
        assert_eq!(argv, Value::Bytes(addr.serialize()));

        let mut flag = Value::U8(1);
        let err = FuncArgvTypes::from_types(None, vec![ValueTy::Bool])
            .unwrap()
            .check_params(&mut flag)
            .unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
        assert_eq!(flag, Value::U8(1));
    }

    #[test]
    fn check_params_multi_compo_list_is_rejected_without_mutation() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::U8, ValueTy::U8]).unwrap();
        let shared = CompoItem::list(VecDeque::from([Value::U16(1), Value::U16(256)])).unwrap();
        let mut argv = Value::Compo(shared.clone());
        let alias = Value::Compo(shared);
        let snapshot = |v: &Value| -> Vec<Value> {
            v.compo_ref()
                .unwrap()
                .list_ref()
                .unwrap()
                .iter()
                .cloned()
                .collect()
        };

        assert_eq!(snapshot(&argv), vec![Value::U16(1), Value::U16(256)]);
        assert_eq!(snapshot(&alias), vec![Value::U16(1), Value::U16(256)]);

        let err = types.check_params(&mut argv).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);

        assert_eq!(snapshot(&argv), vec![Value::U16(1), Value::U16(256)]);
        assert_eq!(snapshot(&alias), vec![Value::U16(1), Value::U16(256)]);
    }

    #[test]
    fn check_params_multi_non_tuple_input_uses_call_argv_type_fail() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::U8, ValueTy::U16]).unwrap();
        let mut argv = Value::U8(1);
        let err = types.check_params(&mut argv).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
    }

    #[test]
    fn check_params_multi_map_input_uses_call_argv_type_fail() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::U8, ValueTy::U16]).unwrap();
        let mut argv = Value::Compo(CompoItem::new_map());
        let err = types.check_params(&mut argv).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
    }

    #[test]
    fn check_params_multi_args_input_supports_tuple_only() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::U16, ValueTy::U64]).unwrap();

        let mut args = Value::Tuple(TupleItem::new(vec![Value::U8(1), Value::U16(2)]).unwrap());
        types.check_params(&mut args).unwrap();
        assert_eq!(
            args,
            Value::Tuple(TupleItem::new(vec![Value::U16(1), Value::U64(2)]).unwrap())
        );
    }

    #[test]
    fn check_params_multi_args_rejects_compo_list_input() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::U16, ValueTy::U64]).unwrap();
        let mut list =
            Value::Compo(CompoItem::list(VecDeque::from([Value::U8(1), Value::U16(2)])).unwrap());
        let err = types.check_params(&mut list).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
    }

    #[test]
    fn check_params_single_compo_input_supported() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::Compo]).unwrap();
        let mut argv = Value::Compo(CompoItem::new_map());
        types.check_params(&mut argv).unwrap();
    }

    #[test]
    fn check_params_single_compo_list_input_supported() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::Compo]).unwrap();
        let mut argv = Value::Compo(CompoItem::list(VecDeque::from([Value::U8(1)])).unwrap());
        types.check_params(&mut argv).unwrap();
    }

    #[test]
    fn check_params_multi_args_input_supports_compo_items() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::Compo, ValueTy::U8]).unwrap();
        let mut args = Value::Tuple(
            TupleItem::new(vec![Value::Compo(CompoItem::new_map()), Value::U8(7)]).unwrap(),
        );
        types.check_params(&mut args).unwrap();
    }

    #[test]
    fn check_params_multi_args_supports_compo_list_as_ordinary_item() {
        let types = FuncArgvTypes::from_types(None, vec![ValueTy::Compo, ValueTy::U8]).unwrap();
        let mut args = Value::Tuple(
            TupleItem::new(vec![
                Value::Compo(CompoItem::list(VecDeque::from([Value::U8(1)])).unwrap()),
                Value::U8(7),
            ])
            .unwrap(),
        );
        types.check_params(&mut args).unwrap();
    }

    #[test]
    fn check_output_normalizes_uints() {
        let tys = FuncArgvTypes::from_types(Some(ValueTy::U64), vec![]).unwrap();
        let mut out = Value::U16(7);
        tys.check_output(&mut out).unwrap();
        assert_eq!(out, Value::U64(7));
    }

    #[test]
    fn check_output_rejects_cross_family_casts() {
        let tys = FuncArgvTypes::from_types(Some(ValueTy::Bool), vec![]).unwrap();
        let mut out = Value::U16(7);
        let err = tys.check_output(&mut out).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
        assert_eq!(out, Value::U16(7));
    }

    #[test]
    fn check_output_uses_call_argv_type_fail_for_unreachable_target() {
        let tys = FuncArgvTypes::from_types(Some(ValueTy::Compo), vec![]).unwrap();
        let mut out = Value::U8(1);
        let err = tys.check_output(&mut out).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
    }

    #[test]
    fn args_output_type_roundtrips_and_checks_exact_match() {
        let tys = FuncArgvTypes::from_types(Some(ValueTy::Tuple), vec![]).unwrap();
        assert_eq!(tys.output_type().unwrap(), Some(ValueTy::Tuple));

        let mut out = Value::Tuple(
            TupleItem::new(vec![Value::U8(1), Value::Compo(CompoItem::new_map())]).unwrap(),
        );
        tys.check_output(&mut out).unwrap();
    }

    #[test]
    fn args_output_rejects_non_args_value() {
        let tys = FuncArgvTypes::from_types(Some(ValueTy::Tuple), vec![]).unwrap();
        let mut out = Value::U8(1);
        let err = tys.check_output(&mut out).unwrap_err();
        assert_eq!(err.0, ItrErrCode::CallArgvTypeFail);
    }
}
