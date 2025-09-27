//! Definitions of the [Workspace] and [WorkspaceBuilder] types. The [Workspace]
//! is responsible for orchestrating and storing all of the required metadata
//! and information about a particular lint run. The [WorkspaceBuilder] is
//! responsible for creating a [Workspace] instance.

mod member;
pub mod resolver;
pub mod settings;

use std::{collections::HashMap, path::PathBuf};

use bl_ast::{HasSource, LineRanges, SourceId};
use bl_utils::stream::CompilerOutputStream;
use index_vec::IndexVec;
pub use member::Member;
use member::MemberSourceMetadata;
use settings::Settings;

#[derive(Default, Debug)]
pub struct WorkspaceMembers {
    member_map: HashMap<PathBuf, SourceId>,

    /// All of the members in the [Workspace].
    members: IndexVec<SourceId, Member>,
}

impl WorkspaceMembers {
    pub fn new() -> Self {
        WorkspaceMembers { member_map: HashMap::new(), members: IndexVec::new() }
    }

    /// Reserve a member ready for setting its contents after it has completed
    /// the first stage, i.e. the parsing.
    pub fn reserve_member(&mut self, path: PathBuf, contents: String) -> SourceId {
        // Calculate the "line_map" for the member.
        //
        // @@Future: we could make this a lazy-cell and just load it at time of use.
        let line_map = LineRanges::new_from_str(&contents);
        let metadata = MemberSourceMetadata::new(line_map);

        let id = self.members.push(Member::new(path.clone(), contents, None, metadata));
        self.member_map.insert(path, id);

        id
    }

    pub fn member_mut(&mut self, id: SourceId) -> &mut Member {
        &mut self.members[id]
    }

    pub fn member(&self, id: SourceId) -> &Member {
        &self.members[id]
    }

    fn path_to_id(&self, path: &PathBuf) -> Option<SourceId> {
        self.member_map.get(path).copied()
    }

    /// Get a reference to a [Member] by its associated file path.
    pub fn get_member_by_path(&self, path: &PathBuf) -> Option<&Member> {
        let id = self.path_to_id(path)?;
        self.members.get(id)
    }

    /// Get a reference to a member by its [SourceId].
    pub fn get_member_by_id(&self, id: SourceId) -> Option<&Member> {
        self.members.get(id)
    }

    /// Get a mutable reference to a member by its [SourceId].
    pub fn get_member_by_id_mut(&mut self, id: SourceId) -> Option<&mut Member> {
        self.members.get_mut(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (SourceId, &Member)> + '_ {
        self.members.iter_enumerated()
    }
}

impl HasSource for WorkspaceMembers {
    fn contents(&self, source: SourceId) -> &str {
        self.members[source].contents()
    }

    fn path(&self, source: SourceId) -> &str {
        self.members[source].path.to_str().unwrap()
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

impl Workspace {
    pub fn output_stream(&self) -> CompilerOutputStream {
        self.stdout.clone()
    }

    pub fn error_stream(&self) -> CompilerOutputStream {
        self.stderr.clone()
    }
}

impl HasSource for Workspace {
    fn contents(&self, source: SourceId) -> &str {
        self.members.contents(source)
    }

    fn path(&self, source: SourceId) -> &str {
        self.members.path(source)
    }
}

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
            settings: self.settings.unwrap_or_default(),
            members: WorkspaceMembers::new(),
        }
    }
}
