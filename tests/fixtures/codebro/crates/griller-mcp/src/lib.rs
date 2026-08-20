pub struct Tool {
    pub name: &'static str,
}

pub fn register_tool(name: &'static str) -> Tool {
    Tool { name }
}
