#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Three pane browser (the classic "Browser" window).
    Browser,
    /// Single image viewer.
    Viewer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Native folder chooser (macOS) / built-in browser elsewhere.
    OpenFolder,
    /// Hand the current file to the system's default application.
    OpenWithDefault,
    UpFolder,
    GoHome,
    GoPictures,
    Reload,
    MoveToTrash,
    GetInfo,
    Rename,
    MoveToFolder,
    CopyToFolder,
    Quit,

    SelectAll,
    SelectNone,
    InvertSelection,

    NextImage,
    PrevImage,
    FirstImage,
    LastImage,
    OpenViewer,
    CloseViewer,

    ZoomIn,
    ZoomOut,
    FitWindow,
    ActualSize,
    RotateCW,
    RotateCCW,
    FlipH,
    FlipV,

    Slideshow,
    StopSlideshow,
    ToggleFullscreen,

    ThumbSmall,
    ThumbMedium,
    ThumbLarge,
    SortByName,
    SortByDate,
    SortBySize,
    SortByType,
    ToggleReverse,
    ToggleFolders,
    ToggleSidebar,
    ToggleInspector,
    ToggleToolbar,
    ToggleStatus,

    Settings,
    About,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Command dispatch passes these around by value; keep them plain data.
    use std::fmt::Debug;
    fn assert_plain_data<T: Clone + Copy + PartialEq + Eq + Debug>() {}

    #[test]
    fn enums_are_plain_data() {
        assert_plain_data::<Mode>();
        assert_plain_data::<Action>();
    }
}
