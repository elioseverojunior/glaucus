// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Binary entry point. All logic lives in `glaucus::cli`.

fn main() -> std::process::ExitCode {
    glaucus::cli::process::main()
}
