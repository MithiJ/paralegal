// ///THIS example has UserData as a wrapper around i64. So &UserData is equivalent with &i64
// #[paralegal::marker(sensitive)]
// struct UserData {
//     pub data: i64,
// }

// #[paralegal::analyze]
// fn process_basic() {
//     let user_data = UserData {
//         data: 1,
//     };
//     check_user_data(&user_data);
//     // check2(&user_data.data, &user_data.strs, &user_data.val);
// }

// #[paralegal::marker{source, return}]
// fn get_user_data() -> UserData  {
//     return UserData {
//         data: 1,
//     };
// }
// #[paralegal::marker{checks, arguments = [0]}]
// fn check_user_data(user_data: &UserData) -> bool {
//     // for i in &user_data.data {
//         if user_data.data < 0 {
//             return false;
//         }
//     // }
//     return true;
// }
// // #[paralegal::marker{checks, arguments = [0]}]
// // fn check2<'a>(data: &Vec<i64>, strs: &Vec<& 'a str>, val: &u32) -> bool {
// //     for i in data {
// //         if i < &0 {
// //             return false;
// //         }
// //     }
// //     return true;
// // }

// fn main() {}

/// Now we have a case where UserData has multiple fields not just the i64
/// We also have to add a lifetime parameter for str!
#[paralegal::marker(sensitive)]
struct UserData<'a> {
    data: &'a User,
    val: &'a str,
}

struct User {
    data: i64,
    val: i64,
}

#[paralegal::analyze]
fn process_basic(ud: &UserData) {
    // let user_data = UserData {
    //     data: &User {
    //         data: 1,
    //         val: 1,
    //     },
    //     val: "hi",
    // };
    check_user_data(ud);
}

#[paralegal::marker{source, return}]
fn get_user_data<'a>() -> UserData<'a>  {
    return UserData {
        data: &User {
            data: 1,
            val: 1,
        },
        val: "hi",
    };
}
#[paralegal::marker{checks, arguments = [0]}]
fn check_user_data<'a>(user_data: &UserData<'a>) -> bool {
    // for i in &user_data.data {
        if (*user_data.data).data < 0 {
            return false;
        }
    // }
    return true;
}
// #[paralegal::marker{checks, arguments = [0]}]
// fn check2<'a>(data: &Vec<i64>, strs: &Vec<& 'a str>, val: &u32) -> bool {
//     for i in data {
//         if i < &0 {
//             return false;
//         }
//     }
//     return true;
// }

fn main() {}