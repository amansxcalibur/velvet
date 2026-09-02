#[derive(Debug)]
pub enum WindowRole {
    Toplevel,
    Dialog,
    Popup,
}

#[derive(Debug)]
pub struct WindowDescriptor {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub role: WindowRole,
    pub decorated: bool,
}

impl Default for WindowDescriptor {
    fn default() -> Self {
        Self {
            title: "Velvet Application".into(),
            width: 800,
            height: 600,
            role: WindowRole::Toplevel,
            decorated: true,
        }
    }
}
