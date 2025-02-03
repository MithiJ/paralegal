//! Stores
//!
//! db/
//!  |-- img/
//!  |    |-- username.filename.jpg
//!  |-- doc/
//!       |-- username.docname.txt

struct User {
    name: String,
}

#[paralegal::marker(user_data)]
struct Image {
    user: User,
    name: String,
}
// TODOM: Just have one integer!

impl Image {
    fn for_user(user: &User) {}
    #[paralegal::marker(sink, return)] 
    // This is not exactly correct but replicating ext annotations?
    fn delete(&mut self) {}
    fn modify_image(&mut self) {}
}
#[paralegal::marker(source, return)]
fn get_from_database(database:&str, _id:&str) -> Image { 
    Image {
            user: User{
                name: "dummyDB".to_string()
            },
            name: "dummyDB".to_string(),
        }
    }

const experimental_value: bool = false;

#[paralegal::analyze]
fn delete() -> u32 {
    let mut x = 1;
    if experimental_value {
        x += 1
    }
    x;

    return 10
}

fn main() {
    let mut args = std::env::args().skip(1);

    delete();
    
}
