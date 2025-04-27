struct User {
    name: String,
}

#[paralegal::marker(user_data)]
struct Image {
    user: User,
    name: String,
}

impl Image {
    fn for_user(user: &User) {}
    #[paralegal::marker(sink, return)] 
    // This is not exactly correct but replicating ext annotations?
    fn delete(&mut self) {}
    fn modify_image(&mut self) {}
}
// Markers -
// complete description of the function and don't look inside
// implicit assumption that the marker + type describe the function
// an exemption function vs having paralegal analys
#[paralegal::marker(source, return)]
fn get_from_database(database:&str, _id:&str) -> Image { 
    Image {
            user: User{
                name: "dummy".to_string()
            },
            name: "dummy".to_string(),
        }
}

const experimental_value: bool = false;

// #[paralegal::analyze]
fn delete_without_args() {
    let mut img = get_from_database("database", "id");
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
// #[paralegal::analyze]
// fn deletes_in_both_cases(img: &mut Image) {
//     if experimental_value  {
//         *img = get_from_database("database", img.name.as_str())
//     } else {
//         *img = Image {
//             user: User{
//                 name: "dummy".to_string()
//             },
//             name: "dummy".to_string(),
//         }
//     }
//     img.delete()
// }
// Key - Img.delete() overestimates a dependency on img from both branches
// 

#[paralegal::analyze]
fn deletes_in_dummy_case(img: &mut Image) {
    if experimental_value  {
        *img = get_from_database("database", img.name.as_str())
    } else {
        *img = Image {
            user: User{
                name: "dummy".to_string()
            },
            name: "dummy".to_string(),
        };
        img.delete();
    }
}
// Note: Here there is a case for policy refinement because if we say the image
// from the database should - No tentativeness. 
// #[paralegal::analyze]
// fn conditional_modification(img: &mut Image) {
//     *img = get_from_database("database", img.name.as_str());
//     if experimental_value  {
//         *img = Image {
//             user: User{
//                 name: "dummy".to_string()
//             },
//             name: "dummy".to_string(),
//         }
//     }
//     img.delete()
// }

// // We know that the loop never executes so the DB image does get deleted
// // Tentativeness comes from the fact that if the loop did execute (which we 
// // cannot know statically), then we would be sending a dummy value to the
// // delete sink as in the above cases.
// #[paralegal::analyze]
// fn modified_in_loop(img: &mut Image) {
//     *img = get_from_database("database", img.name.as_str());
//     let vec: Vec<i32> = vec![];
//     for value in vec {
//         *img = Image {
//             user: User{
//                 name: "dummy".to_string()
//             },
//             name: "dummy".to_string(),
//         }
//     }
//     img.delete();
// }

// #[paralegal::analyze]
// fn modifying_helper(img: &mut Image) {
//     *img = get_from_database("database", img.name.as_str());
//     img.modify_image();
//     img.delete();
// }

// #[paralegal::analyze]
// fn simple_int_increment() {
//     let mut x = 1;
//     if experimental_value {
//         x += 1
//     }
//     return x
// }

fn main() {}