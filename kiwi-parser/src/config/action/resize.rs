#[derive(Debug)]
pub enum Resize {
    /// Increase width by dx
    IncreaseWidth,
    /// Increase height by dy
    IncreaseHeight,
    /// Increase both width and height by (dx, dy)
    IncreaseBoth,
    /// Decrease width by dx
    DecreaseWidth,
    /// Decrease height by dy
    DecreaseHeight,
    /// Decrease both width and height by (dx, dy)
    DecreaseBoth,
}

impl TryFrom<&str> for Resize {
    type Error = String;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value
            .to_lowercase()
            .replace("_", "")
            .replace(" ", "")
            .as_str()
        {
            "increasewidth" => Ok(Resize::IncreaseWidth),
            "increaseheight" => Ok(Resize::IncreaseHeight),
            "increaseboth" => Ok(Resize::IncreaseBoth),
            "decreasewidth" => Ok(Resize::DecreaseWidth),
            "decreaseheight" => Ok(Resize::DecreaseHeight),
            "decreaseboth" => Ok(Resize::DecreaseBoth),
            _ => Err(format!("Invalid resize: {}", value)),
        }
    }
}

impl Into<&'static str> for Resize {
    fn into(self) -> &'static str {
        match self {
            Resize::IncreaseWidth => "IncreaseWidth",
            Resize::IncreaseHeight => "IncreaseHeight",
            Resize::IncreaseBoth => "IncreaseBoth",
            Resize::DecreaseWidth => "DecreaseWidth",
            Resize::DecreaseHeight => "DecreaseHeight",
            Resize::DecreaseBoth => "DecreaseBoth",
        }
    }
}
