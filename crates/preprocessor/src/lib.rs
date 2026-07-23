pub mod preprocessor;
pub use preprocessor::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_hello_np() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest.parent().unwrap().parent().unwrap();
        let hello = workspace.join("tests/golden/01_basics/hello.np");
        let source = std::fs::read_to_string(&hello).unwrap();
        let include = workspace.join("include").to_str().unwrap().to_string();
        let foundation = workspace.join("include/Foundation").to_str().unwrap().to_string();
        let pre = Preprocessor::process(&source, hello.to_str().unwrap(), &[include, ".".into(), foundation]).unwrap();
        println!("=== NUPA OUT ===");
        println!("{}", pre.resolved_nupa);
        println!("=== C HEADERS ===");
        for h in &pre.c_headers {
            println!("{}", h);
        }
    }
}
