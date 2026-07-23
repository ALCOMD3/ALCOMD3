use anyhow::*;

mod alcom_updater_json;
mod alcomd3_config;
mod build_alcom;
mod bundle_alcom;
mod check_static_link;
mod generate_alcom_updater_key;
mod release_assemble;
mod release_assets;
mod release_build;
mod release_common;
mod release_preflight;
mod release_prepare;
mod release_publish;
mod release_updater;
mod release_validate;
mod sign_alcom_app;
mod sign_alcom_updater;
mod utils;
mod verify_alcom_updater_json;
mod verify_alcom_updater_key;

trait Command {
    fn run(self) -> Result<i32>;
}

macro_rules! commands_def {
    (
        $(
        $(#[$attr:meta])*
        $name: ident = $module: ident;
        )*
    ) => {
        #[derive(clap::Parser)]
        enum Commands {
            $($(#[$attr])* $name($module::Command),)*
        }

        impl Command for Commands {
            fn run(self) -> Result<i32> {
                match self {
                    $(Commands::$name(cmd) => Command::run(cmd),)*
                }
            }
        }
    };
}

commands_def! {
    CheckStaticLink = check_static_link;
    AlcomUpdaterJson = alcom_updater_json;
    BuildAlcom = build_alcom;
    BundleAlcom = bundle_alcom;
    GenerateAlcomUpdaterKey = generate_alcom_updater_key;
    ReleasePrepare = release_prepare;
    ReleasePreflight = release_preflight;
    ReleaseBuild = release_build;
    ReleaseAssemble = release_assemble;
    ReleasePublish = release_publish;
    ReleaseUpdater = release_updater;
    ReleaseValidate = release_validate;
    SignAlcomApp = sign_alcom_app;
    SignAlcomUpdater = sign_alcom_updater;
    VerifyAlcomUpdaterKey = verify_alcom_updater_key;
    VerifyAlcomUpdaterJson = verify_alcom_updater_json;
}

fn main() -> Result<()> {
    let command: Commands = clap::Parser::parse();
    std::process::exit(command.run()?);
}
