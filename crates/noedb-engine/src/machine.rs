//! Apply replicated commands to an [`LsmTree`].

use noedb_planner::{create_index, ExecError};
use noedb_storage::LsmTree;

use crate::command::Command;

/// Build the LSM key for a table cell.
#[must_use]
pub fn row_key(table: &str, row: &str, column: &str) -> Vec<u8> {
    let mut key = table.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(row.as_bytes());
    key.push(0);
    key.extend_from_slice(column.as_bytes());
    key
}

/// Apply one committed command to local storage.
///
/// # Errors
///
/// Storage or planner errors.
pub fn apply_command(tree: &mut LsmTree, cmd: &Command) -> Result<(), ExecError> {
    match cmd {
        Command::Put {
            table,
            row,
            column,
            value,
        } => {
            let key = row_key(table, row, column);
            tree.put(&key, value)?;
        }
        Command::CreateIndex { table, column } => {
            create_index(tree, table, column)?;
        }
    }
    Ok(())
}
