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
