

#[macro_export]
macro_rules! combi_struct {
    ($class:ident, $( $item:ident : $type:ty )+ ) => (
        

        #[derive(Default, Debug, Clone, PartialEq, Eq)]
        pub struct $class {
            $(
                pub $item: $type
            ),+
        }

        impl Parse for $class {
            fn parse_from(&mut self, buf: &mut &[u8]) -> Ret<usize> {
                let mut mv = 0;
                $( mv += self.$item.parse_from(buf)?; )+
                Ok(mv)
            }
        }

        impl Serialize for $class {
            fn serialize_to(&self, out: &mut Vec<u8>) {
                $( self.$item.serialize_to(out); )+
            }
            fn size(&self) -> usize {
                [ $( self.$item.size() ),+ ].iter().sum()
            }
        }

        impl_field_only_new!{$class}

        impl ToJSON for $class {
            fn to_json_fmt(&self, fmt: &JSONFormater) -> String {
                let mut res = String::from("{");
                let mut _first = true;
                $(
                    if !_first { res.push(','); }
                    res.push_str(&format!("\"{}\":{}", stringify!($item), self.$item.to_json_fmt(fmt)));
                    _first = false;
                )+
                res.push('}');
                res
            }
        }

        impl FromJSON for $class {
            fn from_json(&mut self, json_str: &str) -> Ret<()> {
                let pairs = json_split_object(json_str)?;
                for (k, v) in pairs {
                    $(
                        if k == stringify!($item) {
                            self.$item.from_json(v)?;
                        }
                    )+
                }
                Ok(())
            }
        }
    )
}


// test
combi_struct!{ Test73895763489564,
    aaa: Uint1
}









#[macro_export]
macro_rules! combi_struct_with_parse_serialize {
    ($class:ident, ( $this:ident, $buf:ident, $parse:expr, |$out:ident| $serialize_to:block, $size:expr ), $( $item:ident : $type:ty )+ ) => (

        #[derive(Default, Debug, Clone, PartialEq, Eq)]
        pub struct $class {
            $(
                pub $item: $type
            ),+
        }

        impl Parse for $class {
            fn parse(&mut $this, $buf: &[u8]) -> Ret<usize> {
                $parse
            }
        }

        impl Serialize for $class {
            fn serialize_to(&$this, $out: &mut Vec<u8>) {
                $serialize_to
            }
            fn size(&$this) -> usize {
                $size
            }
        }

        impl_field_only_new!{$class}

        impl ToJSON for $class {
            fn to_json_fmt(&self, fmt: &JSONFormater) -> String {
                let mut res = String::from("{");
                let mut _first = true;
                $(
                    if !_first { res.push(','); }
                    res.push_str(&format!("\"{}\":{}", stringify!($item), self.$item.to_json_fmt(fmt)));
                    _first = false;
                )+
                res.push('}');
                res
            }
        }

        impl FromJSON for $class {
            fn from_json(&mut self, json_str: &str) -> Ret<()> {
                let pairs = json_split_object(json_str)?;
                for (k, v) in pairs {
                    $(
                        if k == stringify!($item) {
                            self.$item.from_json(v)?;
                        }
                    )+
                }
                Ok(())
            }
        }

    );
    ($class:ident, ( $this:ident, $buf:ident, $parse:expr, $serialize:expr, $size:expr ), $( $item:ident : $type:ty )+ ) => (

        #[derive(Default, Debug, Clone, PartialEq, Eq)]
        pub struct $class {
            $(
                pub $item: $type
            ),+
        }

        impl Parse for $class {
            fn parse(&mut $this, $buf: &[u8]) -> Ret<usize> {
                $parse
            }
        }

        impl Serialize for $class {
            fn serialize(&$this) -> Vec<u8> {
                $serialize
            }
            fn serialize_to(&$this, out: &mut Vec<u8>) {
                let v = $serialize;
                out.extend_from_slice(&v);
            }
            fn size(&$this) -> usize {
                $size
            }
        }

        impl_field_only_new!{$class}

        impl ToJSON for $class {
            fn to_json_fmt(&self, fmt: &JSONFormater) -> String {
                let mut res = String::from("{");
                let mut _first = true;
                $(
                    if !_first { res.push(','); }
                    res.push_str(&format!("\"{}\":{}", stringify!($item), self.$item.to_json_fmt(fmt)));
                    _first = false;
                )+
                res.push('}');
                res
            }
        }

        impl FromJSON for $class {
            fn from_json(&mut self, json_str: &str) -> Ret<()> {
                let pairs = json_split_object(json_str)?;
                for (k, v) in pairs {
                    $(
                        if k == stringify!($item) {
                            self.$item.from_json(v)?;
                        }
                    )+
                }
                Ok(())
            }
        }

    )
}


#[macro_export]
macro_rules! combi_struct_with_parse {
    ($class:ident, ( $this:ident, $buf:ident, $parse:expr ), $( $item:ident : $type:ty )+ ) => (

        combi_struct_with_parse_serialize!{$class, (
            $this, $buf, $parse, |out| {
                $( $this.$item.serialize_to(out); )+
            },
            [ $( $this.$item.size() ),+ ].iter().sum()
        ), $( $item : $type )+ }

    )
}






#[macro_export]
macro_rules! combi_struct_field_more_than_condition {
    ($class:ident, { $( $item:ident : $type:ty )+ }, $mrn:ident, $mrv:ty, $cdn:ident, $cdv:expr ) => (

        #[derive(Default, Debug, Clone, PartialEq, Eq)]
        pub struct $class {
            $(
                pub $item: $type,
            )+
            pub $mrn: $mrv
        }

        impl Parse for $class {

            fn parse_from(&mut self, buf: &mut &[u8]) -> Ret<usize> {
                let mut mv = 0;
                $( mv += self.$item.parse_from(buf)?; )+
                if *self.$cdn > $cdv {
                    mv += self.$mrn.parse_from(buf)?;
                }
                Ok(mv)
            }

        }

        impl Serialize for $class {

            fn serialize_to(&self, out: &mut Vec<u8>) {
                $( self.$item.serialize_to(out); )+
                if *self.$cdn > $cdv {
                    self.$mrn.serialize_to(out);
                }
            }

            fn size(&self) -> usize {
                let mut sz = [ $( self.$item.size() ),+ ].iter().sum();
                if *self.$cdn > $cdv {
                    sz += self.$mrn.size();
                }
                sz
            }

        }

        impl_field_only_new!{$class}

        impl ToJSON for $class {
            fn to_json_fmt(&self, fmt: &JSONFormater) -> String {
                let mut res = String::from("{");
                let mut _first = true;
                $(
                    if !_first { res.push(','); }
                    res.push_str(&format!("\"{}\":{}", stringify!($item), self.$item.to_json_fmt(fmt)));
                    _first = false;
                )+
                if *self.$cdn > $cdv {
                    if !_first { res.push(','); }
                    res.push_str(&format!("\"{}\":{}", stringify!($mrn), self.$mrn.to_json_fmt(fmt)));
                }
                res.push('}');
                res
            }
        }

        impl FromJSON for $class {
            fn from_json(&mut self, json_str: &str) -> Ret<()> {
                let pairs = json_split_object(json_str)?;
                for (k, v) in &pairs {
                    $(
                        if *k == stringify!($item) {
                            self.$item.from_json(v)?;
                        }
                    )+
                }
                if *self.$cdn <= $cdv {
                    self.$mrn = <$mrv>::default();
                }
                for (k, v) in pairs {
                    if k == stringify!($mrn) && *self.$cdn > $cdv {
                        self.$mrn.from_json(v)?;
                    }
                }
                Ok(())
            }
        }

    )
}
