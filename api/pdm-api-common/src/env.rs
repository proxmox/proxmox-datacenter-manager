pub fn sanitize_environment_vars() {
    std::env::set_var("PATH", "/sbin:/bin:/usr/sbin:/usr/bin");
    // Make %ENV safer - as suggested by https://perldoc.perl.org/perlsec.html
    for name in &["IFS", "CDPATH", "ENV", "BASH_ENV"] {
        std::env::remove_var(name);
    }
}
