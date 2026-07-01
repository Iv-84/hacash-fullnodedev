
#[derive(Clone, Copy)]
pub struct ServerConf {
    pub enable: bool,
    pub listen: u16,
    pub multi_thread: bool,
    pub debug_open: bool,
}

#[cfg(test)]
mod server_conf_tests {
    use super::*;

    #[test]
    fn debug_open_defaults_to_false() {
        let ini = IniObj::new();
        let cnf = ServerConf::new(&ini);
        assert!(!cnf.debug_open);
    }

    #[test]
    fn debug_open_reads_server_section() {
        let mut ini = IniObj::new();
        let mut server = std::collections::HashMap::new();
        server.insert("debug_open".to_owned(), Some("true".to_owned()));
        ini.insert("server".to_owned(), server);

        let cnf = ServerConf::new(&ini);
        assert!(cnf.debug_open);
    }
}



impl  ServerConf {
    
    pub fn new(ini: &IniObj) -> ServerConf {
        let sec = ini_section(ini, "server");
        let cnf = ServerConf{
            enable:       ini_must_bool(&sec, "enable", false),
            listen:   ini_must_u64(&sec, "listen", 8083) as u16,
            multi_thread: ini_must_bool(&sec, "multi_thread", false),
            debug_open: ini_must_bool(&sec, "debug_open", false),
        };

        cnf
    }


}
