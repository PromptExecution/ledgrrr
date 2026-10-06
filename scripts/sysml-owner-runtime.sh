#!/usr/bin/env bash
# Task-owned runtime. Never operate on the retained pex-sysml-reference pod.
set -euo pipefail
umask 077
STATE=${SYSML_OWNER_STATE:-/tmp/sysml-owner-c9d}
SOURCE=${SYSML_REFERENCE_SOURCE:-/home/brianh/promptexecution/.worktrees/sysml-v2-api-reference}
PIN=0af711b14bbcea7b240bb0a3a65817ae68302092
JAVA=docker.io/library/eclipse-temurin@sha256:8db2bcf62ae171d1247c331db0410d8bc6347e7493bdafe104bb9a33d59c1291
POSTGRES=docker.io/library/postgres@sha256:1a6ab3f5345eb6dbe04a1349529caabdb0ab09293a09590fad07b2246bfa4b54
POD=pex-sysml-owner-c9d
DB=$POD-db
API=$POD-native
OWNER=$POD-owner
PORT=19001
unknown() { printf 'Unknown: %s\n' "$*" >&2; exit 2; }
command -v podman >/dev/null || unknown 'Podman is unavailable'
mkdir -p "$STATE"
chmod 700 "$STATE"
# One state directory and one physical owner per runtime. No automatic reset.
exec 9>"$STATE/runtime.lock"
flock -n 9 || unknown 'another runtime controller is active'
# Conmon must not inherit the controller lock into its daemon lifetime.
podman() { command podman "$@" 9>&-; }
api() { podman exec "$API" curl --silent --show-error --fail --max-time 15 "$@"; }
provider_up() {
    test "$(git -C "$SOURCE" rev-parse HEAD)" = "$PIN" || unknown 'reference source pin differs'
    test -x "$SOURCE/target/universal/stage/bin/sysml-v2-api-services" || unknown 'pinned reference stage is absent; build it independently first'
    if ! test -d "$STATE/app"; then
        cp -a "$SOURCE/target/universal/stage" "$STATE/app"
        python3 - "$STATE/app/conf/META-INF/persistence.xml" <<'PY'
import sys,pathlib
p=pathlib.Path(sys.argv[1]); text=p.read_text()
assert 'name="hibernate.hbm2ddl.auto" value="create-drop"' in text
text=text.replace('name="hibernate.hbm2ddl.auto" value="create-drop"', 'name="hibernate.hbm2ddl.auto" value="update"')
text=text.replace('name="hibernate.show_sql" value="true"', 'name="hibernate.show_sql" value="false"')
p.write_text(text)
PY
    fi
    # Refuse a task copy with destructive schema settings, including accidental edits.
    python3 - "$STATE/app/conf/META-INF/persistence.xml" <<'PY'
import sys, xml.etree.ElementTree as E
props={x.get('name'):x.get('value') for x in E.parse(sys.argv[1]).iter() if x.tag.endswith('property')}
assert props.get('hibernate.hbm2ddl.auto')=='update', 'preserving schema configuration required'
PY
    if ! podman pod exists "$POD"; then
        podman pod create --name "$POD" --publish "127.0.0.1:$PORT:$PORT" >/dev/null
    fi
    # Confirm that an existing pod has exactly our one owner publication.
    podman pod inspect "$POD" > "$STATE/pod.json"
    python3 - "$STATE/pod.json" "$PORT" <<'PY'
import json,sys
p=json.load(open(sys.argv[1])); p=p[0] if isinstance(p,list) else p; infra=p['InfraConfig']; ports=infra.get('PortBindings',{})
assert set(ports)=={sys.argv[2]+'/tcp'}, 'unexpected pod publication'
assert ports[sys.argv[2]+'/tcp']==[{'HostIp':'127.0.0.1','HostPort':sys.argv[2]}], 'owner must publish only on loopback'
PY
    if ! podman container exists "$DB"; then
        mkdir -p "$STATE/postgres"
        podman run -d --name "$DB" --pod "$POD" --volume "$STATE/postgres:/var/lib/postgresql/data:Z" \
            --env POSTGRES_PASSWORD=mysecretpassword --env POSTGRES_DB=sysml2 "$POSTGRES" \
            postgres -c listen_addresses=127.0.0.1 >/dev/null
    else podman start "$DB" >/dev/null; fi
    for _ in $(seq 1 60); do
        if podman exec "$DB" pg_isready -h 127.0.0.1 -U postgres -d sysml2 >/dev/null 2>&1; then break; fi
        sleep 1
    done
    podman exec "$DB" pg_isready -h 127.0.0.1 -U postgres -d sysml2 >/dev/null || unknown 'database startup timed out'
    if ! podman container exists "$API"; then
        podman run -d --name "$API" --pod "$POD" --volume "$STATE/app:/app:ro,Z" "$JAVA" \
            /app/bin/sysml-v2-api-services -J-Xmx1536m -J-XX:ActiveProcessorCount=2 \
            -Dpidfile.path=/tmp/sysml-owner-native.pid -Dhttp.port=9000 -Dhttp.address=127.0.0.1 \
            -Dplay.http.secret.key=isolated-owner-c9d-local-only -Dplay.filters.hosts.allowed.0=127.0.0.1 >/dev/null
    else podman start "$API" >/dev/null; fi
    for _ in $(seq 1 90); do
        if api http://127.0.0.1:9000/projects >/dev/null 2>&1; then break; fi
        sleep 1
    done
    validate_state_binding
    api http://127.0.0.1:9000/projects > "$STATE/projects-observed.json" || unknown 'native startup timed out'
    if ! test -f "$STATE/config.json"; then
        for logical in live-project fresh-project; do
            api -H 'Content-Type: application/json' -X POST -d "{\"@type\":\"Project\",\"name\":\"owner-c9d-$logical\"}" \
                http://127.0.0.1:9000/projects > "$STATE/$logical.json"
        done
        python3 - "$STATE" <<'PY'
import hashlib,json,pathlib,secrets,sys
s=pathlib.Path(sys.argv[1]); actors=['owner','editor-a','editor-b','reader']
c={'bind':'0.0.0.0:19001','backend_origin':'http://127.0.0.1:9000','store_path':'/owner/store.sqlite','owner_actor':'owner',
   'credentials':[{'token':secrets.token_hex(32),'actor':a} for a in actors], 'projects':[]}
for name in ['live-project','fresh-project']:
    p=json.loads((s/(name+'.json')).read_text()); branch=p['defaultBranch']['@id']
    c['projects'].append({'project':name,'remote_project':p['@id'],'branch':'main','remote_branch':branch,'model_dialect':'SysML-v2',
        'actors':[{'actor':a,'read':True,'propose':a!='reader','administer':a=='owner'} for a in actors]})
(s/'config.json').write_text(json.dumps(c,indent=2)+'\n'); (s/'config.json').chmod(0o600)
(s/'pins.json').write_text(json.dumps({'source':'0af711b14bbcea7b240bb0a3a65817ae68302092',
    'java':'sha256:8db2bcf62ae171d1247c331db0410d8bc6347e7493bdafe104bb9a33d59c1291',
    'postgres':'sha256:1a6ab3f5345eb6dbe04a1349529caabdb0ab09293a09590fad07b2246bfa4b54',
    'schema':'hibernate.hbm2ddl.auto=update','native_origin':'pod-local loopback:9000',
    'stage_sha256':hashlib.sha256(b''.join((str(p.relative_to(s/'app')).encode()+b'\0'+hashlib.sha256(p.read_bytes()).digest()) for p in sorted((s/'app').rglob('*')) if p.is_file())).hexdigest(),
    'persistence_sha256':hashlib.sha256((s/'app/conf/META-INF/persistence.xml').read_bytes()).hexdigest()},indent=2)+'\n')
PY
    fi
}
validate_state_binding() {
    podman inspect "$API" > "$STATE/native-container.json"
    python3 - "$STATE/native-container.json" "$STATE/app" <<'PYBIND'
import json,pathlib,sys
c=json.load(open(sys.argv[1]))[0]
mounts={x['Destination']:x['Source'] for x in c['Mounts']}
assert pathlib.Path(mounts['/app']).resolve()==pathlib.Path(sys.argv[2]).resolve(), 'physical backend already belongs to another owner state directory'
PYBIND
    if podman container exists "$OWNER"; then
        podman inspect "$OWNER" > "$STATE/owner-container.json"
        python3 - "$STATE/owner-container.json" "$STATE" <<'PYBIND'
import json,pathlib,sys
c=json.load(open(sys.argv[1]))[0]
mounts={x['Destination']:x['Source'] for x in c['Mounts']}
assert pathlib.Path(mounts['/owner']).resolve()==pathlib.Path(sys.argv[2]).resolve(), 'independent owner databases for one backend are unsupported'
PYBIND
    fi
}
owner_start() {
    validate_state_binding
    test -f "$STATE/config.json" || unknown 'run provider-up first'
    local binary=${SYSML_OWNER_BINARY:-${CARGO_TARGET_DIR:-/tmp/sysml-implementation/ledgrrr-target}/debug/revision-owner}
    test -x "$binary" || unknown 'owner binary is absent; run sysml-owner-build'
    cp "$binary" "$STATE/revision-owner.next"
    mv -f "$STATE/revision-owner.next" "$STATE/revision-owner"
    if podman container exists "$OWNER"; then podman rm -f "$OWNER" >/dev/null; fi
    local hooks=()
    # Trusted harness-only environment; no client API can enable this.
    if test "${OWNER_TEST_CRASH_AFTER_CREATE:-}" = 1; then hooks+=(--env OWNER_TEST_CRASH_AFTER_CREATE=1); fi
    if test "${OWNER_TEST_AMBIGUOUS_AFTER_CREATE:-}" = 1; then hooks+=(--env OWNER_TEST_AMBIGUOUS_AFTER_CREATE=1); fi
    podman run -d --name "$OWNER" --pod "$POD" --volume "$STATE:/owner:Z" "${hooks[@]}" "$JAVA" \
        /owner/revision-owner --config /owner/config.json >/dev/null
    for _ in $(seq 1 30); do
        if curl --silent --max-time 2 -o /dev/null "http://127.0.0.1:$PORT/v1/projects/live-project/branches/main/head"; then
            printf 'Owner endpoint http://127.0.0.1:%s; private config %s/config.json\n' "$PORT" "$STATE"
            return
        fi
        sleep 1
    done
    unknown 'owner startup timed out; inspect task-owned owner logs'
}
case ${1:-up} in
    provider-up) provider_up ;;
    up) provider_up; owner_start ;;
    owner-start|owner-restart) owner_start ;;
    native-restart) podman restart "$API" >/dev/null; for _ in $(seq 1 90); do if api http://127.0.0.1:9000/projects > "$STATE/projects-after-restart.json" 2>/dev/null; then exit 0; fi; sleep 1; done; unknown 'native restart timed out' ;;
    inspect) podman pod inspect "$POD"; podman inspect "$API" "$DB" "$OWNER" ;;
    *) unknown 'supported commands: up, provider-up, owner-start, owner-restart, native-restart, inspect' ;;
esac
