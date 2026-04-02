pub struct C {
    pub bld: &'static str,
    pub dim: &'static str,
    pub cyn: &'static str,
    pub ylw: &'static str,
    pub grn: &'static str,
    pub rst: &'static str,
}

impl C {
    pub fn on() -> Self {
        C {
            bld: "\x1b[1m",
            dim: "\x1b[2m",
            cyn: "\x1b[36m",
            ylw: "\x1b[33m",
            grn: "\x1b[32m",
            rst: "\x1b[0m",
        }
    }
    pub fn off() -> Self {
        C { bld: "", dim: "", cyn: "", ylw: "", grn: "", rst: "" }
    }
}
