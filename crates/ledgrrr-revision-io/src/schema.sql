CREATE TABLE metadata (singleton INTEGER PRIMARY KEY CHECK(singleton=1), schema_digest TEXT NOT NULL) STRICT;
CREATE TABLE projects (project TEXT PRIMARY KEY, binding BLOB NOT NULL, bootstrap_owner TEXT NOT NULL) STRICT;
CREATE TABLE grants (project TEXT NOT NULL REFERENCES projects(project), actor TEXT NOT NULL,
 read INTEGER NOT NULL CHECK(read IN (0,1)), propose INTEGER NOT NULL CHECK(propose IN (0,1)),
 administer INTEGER NOT NULL CHECK(administer IN (0,1)), PRIMARY KEY(project,actor)) STRICT;
CREATE TABLE branches (project TEXT NOT NULL REFERENCES projects(project), branch TEXT NOT NULL,
 remote_branch TEXT NOT NULL, fence INTEGER NOT NULL DEFAULT 0 CHECK(fence>=0),
 active_operation TEXT, PRIMARY KEY(project,branch), UNIQUE(project,remote_branch)) STRICT;
CREATE TABLE artifacts (digest TEXT PRIMARY KEY, bytes BLOB NOT NULL) STRICT;
CREATE TABLE project_artifacts (project TEXT NOT NULL REFERENCES projects(project),
 digest TEXT NOT NULL REFERENCES artifacts(digest), PRIMARY KEY(project,digest)) STRICT;
CREATE TABLE operations (project TEXT NOT NULL, operation TEXT NOT NULL, branch TEXT NOT NULL,
 actor TEXT NOT NULL, expected_head BLOB NOT NULL, proposal_digest TEXT NOT NULL,
 generation INTEGER NOT NULL CHECK(generation>=0), record BLOB NOT NULL,
 PRIMARY KEY(project,operation), FOREIGN KEY(project,branch) REFERENCES branches(project,branch)) STRICT;
CREATE INDEX recovery_order ON operations(project,branch,operation);
CREATE TABLE physical_projects (provider TEXT NOT NULL, remote_project TEXT NOT NULL,
 project TEXT NOT NULL UNIQUE REFERENCES projects(project), PRIMARY KEY(provider,remote_project)) STRICT;
CREATE TABLE accepted_revisions (sequence INTEGER PRIMARY KEY,
 project TEXT NOT NULL REFERENCES projects(project), revision TEXT NOT NULL, branch TEXT NOT NULL,
 parent TEXT, envelope TEXT NOT NULL REFERENCES artifacts(digest), candidate_digest TEXT NOT NULL,
 UNIQUE(project,revision), FOREIGN KEY(project,branch) REFERENCES branches(project,branch)) STRICT;
CREATE TABLE index_jobs (project TEXT NOT NULL, revision TEXT NOT NULL, projection_schema TEXT NOT NULL,
 dialect TEXT NOT NULL, state TEXT NOT NULL CHECK(state IN ('queued','claimed','sealed','published')),
 worker TEXT, fence INTEGER NOT NULL DEFAULT 0 CHECK(fence>=0), lease_until INTEGER NOT NULL DEFAULT 0,
 attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts>=0), descriptor BLOB,
 PRIMARY KEY(project,revision,projection_schema),
 FOREIGN KEY(project,revision) REFERENCES accepted_revisions(project,revision)) STRICT;
CREATE TABLE graph_artifacts (digest TEXT PRIMARY KEY, bytes BLOB NOT NULL) STRICT;
CREATE TABLE branch_index (project TEXT NOT NULL, branch TEXT NOT NULL, model_revision TEXT,
 model_sequence INTEGER NOT NULL DEFAULT 0, unavailable_reason TEXT,
 PRIMARY KEY(project,branch), FOREIGN KEY(project,branch) REFERENCES branches(project,branch)) STRICT;
CREATE TABLE branch_checkpoints (project TEXT NOT NULL, branch TEXT NOT NULL, projection_schema TEXT NOT NULL,
 revision TEXT NOT NULL, sequence INTEGER NOT NULL,
 PRIMARY KEY(project,branch,projection_schema), FOREIGN KEY(project,branch) REFERENCES branches(project,branch)) STRICT;
