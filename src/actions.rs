use crate::platform::MemSnapshot;

#[derive(Debug, Clone)]
pub enum Action {
    Quit,
    Refresh,
    NextTab,
    PrevTab,
    SelectTab(usize),
    UpdateSnapshot(MemSnapshot),
}
