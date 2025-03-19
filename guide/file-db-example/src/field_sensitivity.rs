/// Field Sensitivity tentativeness example 1
/// We have Image _2 with 2.0 and 2.1 which are both string. Either 2.0 or 2.1
/// can flow into delete sink based on control flow. We want to show 2 tentative
/// edges but instead, we find that 2 different _9 nodes flow in, they are 
/// distinct and thus, we don't attach tentativeness.

#[paralegal::marker(user_data)]
struct Image {
    name1: String,
    name2: String,
}

#[paralegal::marker(sink, return)] 
fn delete_sink(name:String) {
    todo!()
}


const experimental_value: bool = false;

#[paralegal::analyze]
fn delete(user: User) {
    let img = Image {
        name1: "dummy1".to_string(),
        name2: "dummy2".to_string(),
    };
    let target = if experimental_value {
        img.name1
    } else { 
        img.name2 
    };
    delete_sink(target)
}

fn main() {
    let mut args = std::env::args().skip(1);

    match args.next().unwrap().as_str() {
        "delete" => delete(User {
            name: args.next().unwrap(),
        }),
        other => panic!("Command not implemented {other}"),
    }
}


/// Nested struct String example
/// In fact it turns out that all raw strings are marked as one place. So, when
/// the strings are _15.0 and _15.1.0 they still get marked distinct and certain

struct User {
    name: String,
}

#[paralegal::marker(user_data)]
struct Image {
    user: User,
    name : String,
}

impl Image {
    fn for_user(user: &User) -> Vec<Self> {
        todo!()
    }
}
#[paralegal::marker(sink, return)] 
fn delete_sink(name:String) {
    todo!()
}

#[paralegal::marker(source, return)]
fn get_from_database(database:&str) -> Image { 
    todo!()
}

const experimental_value: bool = false;

#[paralegal::analyze]
fn delete(user: User) {
    let img = Image {
        user: User {
            name: "dummy1".to_string(),
        },
        name: "dummy2".to_string(),
    };
    let target = if experimental_value {
        img.user.name
    } else { 
        img.name 
    };
    delete_sink(target)
}

fn main() {
    let mut args = std::env::args().skip(1);

    match args.next().unwrap().as_str() {
        "delete" => delete(User {
            name: args.next().unwrap(),
        }),
        other => panic!("Command not implemented {other}"),
    }
}