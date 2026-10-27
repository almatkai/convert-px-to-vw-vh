//! Device names and groups transcribed verbatim from the pinned original.
#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub group: &'static str,
    pub value: &'static str,
    pub title: &'static str,
}
pub const WIDTH: &[Preset] = &[
    Preset {
        group: "Desktop",
        value: "1440",
        title: "Desktop - 1440px",
    },
    Preset {
        group: "Desktop",
        value: "1512",
        title: "MacBook Pro 14\" - 1512px",
    },
    Preset {
        group: "Desktop",
        value: "1728",
        title: "MacBook Pro 16\" - 1728px",
    },
    Preset {
        group: "Tablet",
        value: "1024",
        title: "iPad Pro 12.9\" - 1024px",
    },
    Preset {
        group: "Tablet",
        value: "834",
        title: "iPad Pro 11\" - 834px",
    },
    Preset {
        group: "Tablet",
        value: "744",
        title: "iPad mini 8.3 - 744px",
    },
    Preset {
        group: "Mobile",
        value: "390",
        title: "iPhone 14 - 390px",
    },
    Preset {
        group: "Mobile",
        value: "393",
        title: "iPhone 14 Pro - 393px",
    },
    Preset {
        group: "Mobile",
        value: "430",
        title: "iPhone 14 Pro Max - 430px",
    },
    Preset {
        group: "Mobile",
        value: "375",
        title: "iPhone 13 mini - 375px",
    },
    Preset {
        group: "Mobile",
        value: "320",
        title: "iPhone SE - 320px",
    },
];
pub const HEIGHT: &[Preset] = &[
    Preset {
        group: "Desktop",
        value: "1024",
        title: "Desktop - 1024px",
    },
    Preset {
        group: "Desktop",
        value: "982",
        title: "MacBook Pro 14\" - 982px",
    },
    Preset {
        group: "Desktop",
        value: "1117",
        title: "MacBook Pro 16\" - 1117px",
    },
    Preset {
        group: "Tablet",
        value: "1366",
        title: "iPad Pro 12.9\" - 1366px",
    },
    Preset {
        group: "Tablet",
        value: "1194",
        title: "iPad Pro 11\" - 1194px",
    },
    Preset {
        group: "Tablet",
        value: "1133",
        title: "iPad mini 8.3 - 1133px",
    },
    Preset {
        group: "Mobile",
        value: "844",
        title: "iPhone 14 - 844px",
    },
    Preset {
        group: "Mobile",
        value: "852",
        title: "iPhone 14 Pro - 852px",
    },
    Preset {
        group: "Mobile",
        value: "932",
        title: "iPhone 14 Pro Max - 932",
    },
    Preset {
        group: "Mobile",
        value: "812",
        title: "iPhone 13 mini - 812px",
    },
    Preset {
        group: "Mobile",
        value: "568",
        title: "iPhone SE - 568px",
    },
];
pub fn for_property(property: &str) -> Option<&'static [Preset]> {
    match property {
        "width" => Some(WIDTH),
        "height" => Some(HEIGHT),
        _ => None,
    }
}
