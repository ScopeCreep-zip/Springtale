pub mod list_dir;
pub mod read_file;
pub mod write_file;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the filesystem connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        read_file::declaration(),
        write_file::declaration(),
        list_dir::declaration(),
    ]
}
