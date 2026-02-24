#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Snap {
    // full width x full height
    /// `(0, 0, width, height)`
    Maximize,
    /// `(px, py, width - px, height - py)`
    AlmostMaximize,
    /// `(0, _, width, _)`
    MaximizeWidth,
    /// `(_, 0, _, height)`
    MaximizeHeight,
    /// Toggle fullscreen
    Fullscreen,

    // 1/2 width x full height
    /// `(0, 0, width/2, height)`
    LeftHalf,
    /// `(width/2, 0, width/2, height)`
    CenterHalf,
    /// `(width, 0, width/2, height)`
    RightHalf,

    // 1/3 width x full height
    /// `(0, 0, width/3, height)`
    FirstThird,
    /// `(width/3, 0, width/3, height)`
    CenterThird,
    /// `(2*width/3, 0, width/3, height)`
    LastThird,

    // 1/4 width x full height
    /// `(0, 0, width/4, height)`
    FirstFourth,
    /// `(width/4, 0, width/4, height)`
    SecondFourth,
    /// `(2*width/4, 0, width/4, height)`
    ThirdFourth,
    /// `(3*width/4, 0, width/4, height)`
    LastFourth,

    // full width x 1/2 height
    /// `(0, 0, width, height/2)`
    TopHalf,
    /// `(0, height/2, width, height/2)`
    MiddleHalf,
    /// `(0, height, width, height/2)`
    BottomHalf,

    // full width x 1/3 height
    /// `(0, 0, width, height/3)`
    TopThird,
    /// `(0, height/3, width, height/3)`
    MiddleThird,
    /// `(0, 2*height/3, width, height/3)`
    BottomThird,

    // 1/2 width x 1/2 height
    /// `(0, 0, width/2, height/2)`
    TopLeftQuarter,
    /// `(width/2, 0, width/2, height/2)`
    TopCenterQuarter,
    /// `(width, 0, width/2, height/2)`
    TopRightQuarter,

    /// `(0, height/2, width/2, height/2)`
    MiddleLeftQuarter,
    /// `(width/2, height/2, width/2, height/2)`
    MiddleRightQuarter,

    /// `(width, height/2, width/2, height/2)`
    BottomLeftQuarter,
    /// `(width/2, height, width/2, height/2)`
    BottomCenterQuarter,
    /// `(width, height, width/2, height/2)`
    BottomRightQuarter,

    // 1/3 width x 1/2 height
    /// `(0, 0, width/6, height/2)`
    TopLeftSixth,
    /// `(width/6, 0, width/6, height/2)`
    TopCenterSixth,
    /// `(2*width/6, 0, width/6, height/2)`
    TopRightSixth,

    /// `(0, height/2, width/6, height/2)`
    MiddleLeftSixth,
    /// `(width/6, height/2, width/6, height/2)`
    MiddleCenterSixth,
    /// `(2*width/6, height/2, width/6, height/2)`
    MiddleRightSixth,

    /// `(0, height, width/6, height/2)`
    BottomLeftSixth,
    /// `(width/6, height, width/6, height/2)`
    BottomCenterSixth,
    /// `(2*width/6, height, width/6, height/2)`
    BottomRightSixth,

    // Edge snapping (preserves size)
    /// `(0, _, _, _)`
    Left,
    /// `(width - $0, _, _, _)`
    Right,
    /// `(_, 0, _, _)`
    Top,
    /// `(_, height - $0, _, _)`
    Bottom,
    /// Restores initial bbox
    Restore,
}

impl TryFrom<&str> for Snap {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value
            .to_lowercase()
            .replace("_", "")
            .replace(" ", "")
            .as_str()
        {
            "maximize" => Ok(Snap::Maximize),
            "almostmaximize" => Ok(Snap::AlmostMaximize),
            "maximizewidth" => Ok(Snap::MaximizeWidth),
            "maximizeheight" => Ok(Snap::MaximizeHeight),
            "fullscreen" => Ok(Snap::Fullscreen),
            "lefthalf" => Ok(Snap::LeftHalf),
            "centerhalf" => Ok(Snap::CenterHalf),
            "righthalf" => Ok(Snap::RightHalf),
            "firstthird" => Ok(Snap::FirstThird),
            "centerthird" => Ok(Snap::CenterThird),
            "lastthird" => Ok(Snap::LastThird),
            "firstfourth" => Ok(Snap::FirstFourth),
            "secondfourth" => Ok(Snap::SecondFourth),
            "thirdfourth" => Ok(Snap::ThirdFourth),
            "lastfourth" => Ok(Snap::LastFourth),
            "tophalf" => Ok(Snap::TopHalf),
            "middlehalf" => Ok(Snap::MiddleHalf),
            "bottomhalf" => Ok(Snap::BottomHalf),
            "topthird" => Ok(Snap::TopThird),
            "middlethird" => Ok(Snap::MiddleThird),
            "bottomthird" => Ok(Snap::BottomThird),
            "topleftquarter" => Ok(Snap::TopLeftQuarter),
            "topcenterquarter" => Ok(Snap::TopCenterQuarter),
            "toprightquarter" => Ok(Snap::TopRightQuarter),
            "middleleftquarter" => Ok(Snap::MiddleLeftQuarter),
            "middlerightquarter" => Ok(Snap::MiddleRightQuarter),
            "bottomleftquarter" => Ok(Snap::BottomLeftQuarter),
            "bottomcenterquarter" => Ok(Snap::BottomCenterQuarter),
            "bottomrightquarter" => Ok(Snap::BottomRightQuarter),
            "topleftsixth" => Ok(Snap::TopLeftSixth),
            "topcentersixth" => Ok(Snap::TopCenterSixth),
            "toprightsixth" => Ok(Snap::TopRightSixth),
            "middleleftsixth" => Ok(Snap::MiddleLeftSixth),
            "middlecentersixth" => Ok(Snap::MiddleCenterSixth),
            "middlerightsixth" => Ok(Snap::MiddleRightSixth),
            "bottomleftsixth" => Ok(Snap::BottomLeftSixth),
            "bottomcentersixth" => Ok(Snap::BottomCenterSixth),
            "bottomrightsixth" => Ok(Snap::BottomRightSixth),
            "left" => Ok(Snap::Left),
            "right" => Ok(Snap::Right),
            "top" => Ok(Snap::Top),
            "bottom" => Ok(Snap::Bottom),
            "restore" => Ok(Snap::Restore),
            _ => Err(format!("Invalid snap: {}", value)),
        }
    }
}

impl Into<&'static str> for Snap {
    fn into(self) -> &'static str {
        match self {
            Snap::Maximize => "Maximize",
            Snap::AlmostMaximize => "AlmostMaximize",
            Snap::MaximizeWidth => "MaximizeWidth",
            Snap::MaximizeHeight => "MaximizeHeight",
            Snap::Fullscreen => "Fullscreen",
            Snap::LeftHalf => "LeftHalf",
            Snap::CenterHalf => "CenterHalf",
            Snap::RightHalf => "RightHalf",
            Snap::FirstThird => "FirstThird",
            Snap::CenterThird => "CenterThird",
            Snap::LastThird => "LastThird",
            Snap::FirstFourth => "FirstFourth",
            Snap::SecondFourth => "SecondFourth",
            Snap::ThirdFourth => "ThirdFourth",
            Snap::LastFourth => "LastFourth",
            Snap::TopHalf => "TopHalf",
            Snap::MiddleHalf => "MiddleHalf",
            Snap::BottomHalf => "BottomHalf",
            Snap::TopThird => "TopThird",
            Snap::MiddleThird => "MiddleThird",
            Snap::BottomThird => "BottomThird",
            Snap::TopLeftQuarter => "TopLeftQuarter",
            Snap::TopCenterQuarter => "TopCenterQuarter",
            Snap::TopRightQuarter => "TopRightQuarter",
            Snap::MiddleLeftQuarter => "MiddleLeftQuarter",
            Snap::MiddleRightQuarter => "MiddleRightQuarter",
            Snap::BottomLeftQuarter => "BottomLeftQuarter",
            Snap::BottomCenterQuarter => "BottomCenterQuarter",
            Snap::BottomRightQuarter => "BottomRightQuarter",
            Snap::TopLeftSixth => "TopLeftSixth",
            Snap::TopCenterSixth => "TopCenterSixth",
            Snap::TopRightSixth => "TopRightSixth",
            Snap::MiddleLeftSixth => "MiddleLeftSixth",
            Snap::MiddleCenterSixth => "MiddleCenterSixth",
            Snap::MiddleRightSixth => "MiddleRightSixth",
            Snap::BottomLeftSixth => "BottomLeftSixth",
            Snap::BottomCenterSixth => "BottomCenterSixth",
            Snap::BottomRightSixth => "BottomRightSixth",
            Snap::Left => "Left",
            Snap::Right => "Right",
            Snap::Top => "Top",
            Snap::Bottom => "Bottom",
            Snap::Restore => "Restore",
        }
    }
}
