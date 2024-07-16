//! Definitions of the [Workspace] and [WorkspaceBuilder] types. The [Workspace]
//! is responsible for orchestrating and storing all of the required metadata
//! and information about a particular lint run. The [WorkspaceBuilder] is
//! responsible for creating a [Workspace] instance.

mod member;
pub mod resolver;
pub mod settings;

use std::{collections::HashMap, path::PathBuf};

use bl_lints::settings::FixMode;
use bl_utils::stream::CompilerOutputStream;
use index_vec::IndexVec;
pub use member::{Member, MemberId};
use settings::Settings;

#[derive(Default)]
pub struct WorkspaceMembers {
    member_map: HashMap<PathBuf, MemberId>,

    /// All of the members in the [Workspace].
    members: IndexVec<MemberId, Member>,
}

impl WorkspaceMembers {
    pub fn new() -> Self {
        WorkspaceMembers { member_map: HashMap::new(), members: IndexVec::new() }
    }

    pub fn add_member(&mut self, path: PathBuf, member: Member) -> MemberId {
        let id = self.members.push(member);
        self.member_map.insert(path, id);
        id
    }

    fn path_to_id(&self, path: &PathBuf) -> Option<MemberId> {
        self.member_map.get(path).copied()
    }

    /// Get a reference to a [Member] by its associated file path.
    pub fn get_member_by_path(&self, path: &PathBuf) -> Option<&Member> {
        let id = self.path_to_id(path)?;
        self.members.get(id)
    }

    /// Get a reference to a member by its [MemberId].
    pub fn get_member_by_id(&self, id: MemberId) -> Option<&Member> {
        self.members.get(id)
    }
}

pub struct Workspace {
    /// The [CompilerOutputStream] for `standard output`.
    pub stdout: CompilerOutputStream,

    /// The [CompilerOutputStream] for `standard error`.
    pub stderr: CompilerOutputStream,

    /// The members of the [Workspace].
    pub members: WorkspaceMembers,

    /// The [Settings] for the [Workspace].
    pub settings: Settings,
}

impl Workspace {}

#[derive(Default)]
pub struct WorkspaceBuilder {
    /// Optionally set the [CompilerOutputStream] for `standard output`.
    stdout: Option<CompilerOutputStream>,

    /// Optionally set the [CompilerOutputStream] for `standard error`.
    stderr: Option<CompilerOutputStream>,

    /// Optionally set the [Settings] for the [Workspace].
    settings: Option<Settings>,
}

impl WorkspaceBuilder {
    pub fn new() -> Self {
        WorkspaceBuilder { stdout: None, stderr: None, settings: None }
    }

    pub fn with_stdout(mut self, stdout: CompilerOutputStream) -> Self {
        self.stdout = Some(stdout);
        self
    }

    pub fn with_stderr(mut self, stderr: CompilerOutputStream) -> Self {
        self.stderr = Some(stderr);
        self
    }

    pub fn with_settings(mut self, settings: Settings) -> Self {
        self.settings = Some(settings);
        self
    }

    pub fn build(self) -> Workspace {
        Workspace {
            stdout: self.stdout.unwrap_or_else(CompilerOutputStream::stdout),
            stderr: self.stderr.unwrap_or_else(CompilerOutputStream::stderr),
            settings: self.settings.unwrap_or_else(|| Settings::new(true, FixMode::default())), /* @@Todo: actually
                                                                                                 * create a default
                                                                                                 * for this or
                                                                                                 * something? */
            members: WorkspaceMembers::new(),
        }
    }
}
