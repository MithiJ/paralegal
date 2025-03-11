#![allow(dead_code, unused_variables)]

// struct User {
//     name: String,
// }
// #[paralegal::marker(user_data)]
// struct Image {
//     user: User,
//     name: String,
// }

// impl Image {
//     #[paralegal::marker(sink, return)] 
//     fn delete(self) {
//         todo!()
//     }
// }

// #[paralegal::analyze]
// fn delete(user: User) {
//     let mut img = Image {
//         user: User{
//             name: "dummy".to_string()
//         },
//         name: "dummy".to_string(),
//     };
//     img.delete()
// }

// fn main() {
//     delete(User {
//         name: "dummy".to_string(),
//     })
// }

// // DATA DELETION EXAMPLE

struct User {
    name: String,
}

#[paralegal::marker(user_data)]
struct Image {
    user: User,
    name: String,
}

impl Image {
    fn for_user(user: &User) -> Vec<Self> {
        todo!()
    }

    #[paralegal::marker(sink, return)] 
    fn delete(self) {
        std::fs::remove_file(
            std::path::Path::new("db")
                .join("img")
                .join(format!("{}-{}.jpg", self.user.name, self.name)),
        )
        .unwrap()
    }
}

#[paralegal::marker(source, return)]
fn get_from_database(database:&str) -> Image { 
    Image {
        user: User{
            name: "dummyDB".to_string()
        },
        name: "dummyDB".to_string(),
    }
}

const experimental_value: bool = false;

#[paralegal::analyze]
fn delete(user: User) {
    let mut img = get_from_database("database");
    if experimental_value  {
        img = Image {
            user: User{
                name: "dummy".to_string()
            },
            name: "dummy".to_string(),
        }
    }
    img.delete()
}


#[paralegal::marker(user_data)]
struct Document {
    user: User,
    name: String,
}
impl Document {
    fn for_user(user: &User) -> Vec<Self> {
        todo!()
    }

    fn delete(self) {
        std::fs::remove_file(
            std::path::Path::new("db")
                .join("doc")
                .join(format!("{}-{}.txt", self.user.name, self.name)),
        )
        .unwrap()
    }
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

// SIMPLE INT example
// struct User {
//     name: String,
// }

// #[paralegal::marker(user_data)]
// struct Image {
//     user: User,
//     name: String,
// }


// impl Image {
//     fn for_user(user: &User) {}
//     #[paralegal::marker(sink, return)] 
//     // This is not exactly correct but replicating ext annotations?
//     fn delete(&mut self) {}
//     fn modify_image(&mut self) {}
// }
// #[paralegal::marker(source, return)]
// fn get_from_database(database:&str, _id:&str) -> Image { 
//     Image {
//             user: User{
//                 name: "dummyDB".to_string()
//             },
//             name: "dummyDB".to_string(),
//         }
//     }

// const experimental_value: bool = false;

// #[paralegal::analyze]
// fn delete() -> u32 {
//     let mut x = 1;
//     if false {
//         x += 1
//     }
//     x
// }

// fn main() {
//     let mut args = std::env::args().skip(1);

//     delete();
    
// }
