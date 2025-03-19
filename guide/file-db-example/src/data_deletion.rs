/// This is a classic data deletion example paired with a data deletion policy
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
    #[paralegal::marker(sink, return)] 
    fn delete_sink(name:String) {
        todo!()
    }
}

#[paralegal::marker(source, return)]
fn get_from_database(database:&str) -> Image { 
    todo!()
}

const experimental_value: bool = false;

// Field Sensitivity tentativeness example 1
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

// #[paralegal::marker(user_data)]
// struct Document {
//     user: User,
//     name: String,
// }
// impl Document {
//     fn for_user(user: &User) -> Vec<Self> {
//         todo!()
//     }

//     fn delete(self) {
//         std::fs::remove_file(
//             std::path::Path::new("db")
//                 .join("doc")
//                 .join(format!("{}-{}.txt", self.user.name, self.name)),
//         )
//         .unwrap()
//     }
// }

fn main() {
    let mut args = std::env::args().skip(1);

    match args.next().unwrap().as_str() {
        "delete" => delete(User {
            name: args.next().unwrap(),
        }),
        other => panic!("Command not implemented {other}"),
    }
}