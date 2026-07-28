#[derive(Debug, Clone, Copy)]
pub struct ImagePolicy {
    pub webp_quality: u8,
    pub max_width: u32,
    pub skip_duplicates: bool,
}

impl Default for ImagePolicy {
    fn default() -> Self {
        Self {
            webp_quality: 85,
            max_width: 2560,
            skip_duplicates: true,
        }
    }
}
