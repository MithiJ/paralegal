/// This example has a simple integer modification. We removed all calls to 
/// to_string/string manipulation functions which get counted as un-analyzed 
/// functions taking a mutable reference. Technically, if we were handling 
/// function-based tentativeness, a function that is not inlined should be 
/// assumed to be mutating its mutable arguments for soundness purposes.

const experimental_value: bool = false;

#[paralegal::analyze]
fn delete() -> u32 {
    let mut x = 1;
    if false {
        x += 1
    }
    x
}

fn main() {
    let mut args = std::env::args().skip(1);

    delete();
    
}