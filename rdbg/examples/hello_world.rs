fn main() {
    let world = "world";

    rdbg::msg!("hello {world}");
    rdbg::vals!(world, 1 + 5);
    rdbg::wait_and_quit();
}
