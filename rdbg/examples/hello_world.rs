fn main() {
    let world = "world";

    rdbg::msg!("hello {world}");
    rdbg::flush();
    rdbg::vals!(world, 1 + 5);

    rdbg::msgf!("hello {world}s");
    rdbg::valsf!(world, 2 + 5);
}
