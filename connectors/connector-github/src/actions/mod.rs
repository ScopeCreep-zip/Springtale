pub mod commit_file;
pub mod create_branch;
pub mod create_issue;
pub mod create_pr;
pub mod get_diff;
pub mod post_comment;

#[cfg(test)]
pub mod test_support;

use springtale_connector::manifest::types::ActionDecl;

/// All action declarations for the GitHub connector.
pub fn action_declarations() -> Vec<ActionDecl> {
    vec![
        create_issue::declaration(),
        post_comment::declaration(),
        get_diff::declaration(),
        create_branch::declaration(),
        commit_file::declaration(),
        create_pr::declaration(),
    ]
}
