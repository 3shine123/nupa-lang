#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    lines: Vec<(String, u32)>,
}
impl SourceMap {
    pub fn new(lines: Vec<(String, u32)>) -> Self { SourceMap { lines } }
    pub fn is_empty(&self) -> bool { self.lines.is_empty() }
    pub fn len(&self) -> usize { self.lines.len() }
    pub fn locate(&self, inlined_line: usize) -> (String, u32) {
        if inlined_line > 0 && inlined_line <= self.lines.len() {
            self.lines[inlined_line - 1].clone()
        } else {
            (String::new(), inlined_line as u32)
        }
    }
}
