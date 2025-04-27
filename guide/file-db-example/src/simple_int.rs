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


/// If instead we have to check if some y always depends on x. Here, we see
/// that y depends on the value @ x or the value of some intermediate variable 
/// representing x + 5. That intermediate later depends on x. Thus, a policy
/// would now traverse both flanks and find them originating from x. We can thus
/// say that there is an invariant that y must always depend on x.

const experimental_value: bool = false;

#[paralegal::analyze]
fn delete() -> u32 {
    let mut x = 1;
    let y = if false {
        x + 5 // altered x value
    } else {
        x // actual x value
    }
    y
}

fn main() {
    let mut args = std::env::args().skip(1);
    delete();
}