fn main() {
    cxx_build::bridge("src/lib.rs")
        .file("path/to/nunchuk/cpp/files.cpp")
        .flag_if_supported("-std=c++14")
        .compile("nunchuk");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=path/to/nunchuk/cpp/files.cpp");
}
