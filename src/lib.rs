#![no_std]

/// A simple function that adds two numbers together.
/// This is a test utility function for the expel project.
///
/// # Examples
///
/// ```
/// let result = do_something();
/// assert_eq!(result, 2);
/// ```
pub fn do_something() -> i32 {
    1 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_numbers() {
        let result = do_something();
        assert_eq!(result, 2, "Test utility function should return 2");
    }
}
