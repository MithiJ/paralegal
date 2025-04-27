#[paralegal::marker(sensitive)]
struct UserData {
    pub data: Vec<i64>,
    pub strs: Vec<String>
}

#[paralegal::analyze]
fn process_basic() {
    let user_data = get_user_data();
    check_user_data(&user_data);
}

#[paralegal::marker{source, return}]
fn get_user_data() -> UserData {
    return UserData {
        data: vec![1, 2, 3],
        strs: vec!["hi"]
    };
}
#[paralegal::marker{checks, arguments = [0]}]
fn check_user_data(user_data: &UserData) -> bool {
    for i in &user_data.data {
        if i < &0 {
            return false;
        }
    }
    return true;
}
