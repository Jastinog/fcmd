pub fn file_icon(name: &str, is_dir: bool) -> &'static str {
    if name == ".." {
        return "\u{f005e} "; // 󰁞 arrow up
    }
    if is_dir {
        return match name {
            ".git" => "\u{e702} ",       //
            "node_modules" => "\u{e718} ", //
            "src" => "\u{f0d09} ",        // 󰴉
            "target" => "\u{f487} ",      //
            ".cargo" => "\u{e7a8} ",      //
            _ => "\u{f07b} ",             //
        };
    }

    // Special filenames
    match name {
        "Cargo.toml" | "Cargo.lock" => return "\u{e7a8} ",   //
        "Dockerfile" | "Containerfile" => return "\u{f0868} ", // 󰡨
        "docker-compose.yml" | "docker-compose.yaml" => return "\u{f0868} ",
        ".gitignore" | ".gitmodules" | ".gitattributes" => return "\u{e702} ", //
        "LICENSE" | "LICENSE-MIT" | "LICENSE-APACHE" => return "\u{f0219} ",   // 󰈙
        "Makefile" | "CMakeLists.txt" => return "\u{e779} ",  //
        "README.md" | "README" => return "\u{f00ba} ",        // 󰂺
        ".env" | ".env.local" => return "\u{f462} ",          //
        "package.json" | "package-lock.json" => return "\u{e718} ", //
        "tsconfig.json" => return "\u{e628} ",                //
        _ => {}
    }

    // By extension (case-insensitive)
    let ext_lower = match name.rsplit_once('.') {
        Some((_, e)) => e.to_ascii_lowercase(),
        None => return "\u{f016} ", //
    };

    match ext_lower.as_str() {
        "rs" => "\u{e7a8} ",                                           //
        "js" | "mjs" | "cjs" => "\u{e74e} ",                           //
        "ts" | "mts" | "cts" => "\u{e628} ",                           //
        "jsx" | "tsx" => "\u{e7ba} ",                                   //
        "py" | "pyi" => "\u{e73c} ",                                    //
        "go" => "\u{e724} ",                                            //
        "rb" => "\u{e739} ",                                            //
        "lua" => "\u{e620} ",                                           //
        "c" | "h" => "\u{e61e} ",                                       //
        "cpp" | "cxx" | "cc" | "hpp" => "\u{e61d} ",                    //
        "java" => "\u{e738} ",                                          //
        "swift" => "\u{e755} ",                                         //
        "kt" | "kts" => "\u{e634} ",                                    //
        "zig" => "\u{e6a4} ",                                           //
        "md" | "mdx" => "\u{e73e} ",                                    //
        "json" => "\u{e60b} ",                                          //
        "toml" => "\u{e6b2} ",                                          //
        "yaml" | "yml" => "\u{e6a8} ",                                  //
        "xml" => "\u{f05c0} ",                                          // 󰗀
        "html" | "htm" => "\u{e736} ",                                  //
        "css" | "scss" | "sass" | "less" => "\u{e749} ",                //
        "sh" | "bash" | "zsh" | "fish" => "\u{f489} ",                  //
        "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => "\u{f410} ", //
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "ico" => "\u{f1c5} ", //
        "mp4" | "mkv" | "avi" | "mov" | "webm" => "\u{f03d} ",         //
        "mp3" | "wav" | "flac" | "ogg" | "aac" => "\u{f001} ",         //
        "pdf" => "\u{f1c1} ",                                           //
        "doc" | "docx" => "\u{f0219} ",                                 // 󰈙
        "xls" | "xlsx" => "\u{f021b} ",                                 // 󰈛
        "sql" | "db" | "sqlite" => "\u{f1c0} ",                         //
        "lock" => "\u{f023} ",                                          //
        "log" => "\u{f0331} ",                                          // 󰌱
        "txt" => "\u{f15c} ",                                           //
        "conf" | "cfg" | "ini" => "\u{e615} ",                          //
        "vim" => "\u{e62b} ",                                           //
        "git" => "\u{f1d3} ",                                           //
        _ => "\u{f016} ",                                               //
    }
}
