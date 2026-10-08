#!/usr/bin/env python3
"""Real accepted revisions, sealed publication and rebuild; never reset native data."""
import argparse
import concurrent.futures
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import time
import tomllib
import urllib.parse

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('owner_probe', ROOT / 'scripts/sysml-owner-live-probe.py')
owner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(owner)
STATE, POD = owner.STATE, owner.POD
REPORT = {'status': 'Unknown', 'gates': {}, 'observations': {}, 'limitations': [
    'Owner publication boundary only; kr0ki read integration and interactive UI remain separate gates.',
    'Native library, behavior and compiler resolution are not established by envelope projection.',
    'Host, Podman and database administrators remain trusted.']}
VOCAB = 'urn:ledgrrr:revision:1:'
UNVERIFIED = f'''SELECT ?requirement ?id WHERE {{
 ?requirement <{VOCAB}element_kind> ?kind ; <{VOCAB}id> ?id .
 FILTER(?kind IN ("requirement_definition","requirement_usage"))
 FILTER NOT EXISTS {{ ?relation <{VOCAB}relation_kind> "verify" ;
   <{VOCAB}requirement> ?requirement ; <{VOCAB}authority> "authored" }}
}} ORDER BY ?id'''
SOURCE_PATH = f'''SELECT ?id ?file ?line ?authority WHERE {{
 ?relation <{VOCAB}relation_kind> "satisfy" ; <{VOCAB}requirement> ?requirement ;
   <{VOCAB}subject> ?subject ; <{VOCAB}authority> ?authority .
 ?requirement <{VOCAB}id> ?id . ?subject <{VOCAB}evidence> ?evidence .
 ?evidence <{VOCAB}anchor_kind> "rust_span" ; <{VOCAB}file> ?file ; <{VOCAB}line> ?line .
}} ORDER BY ?id'''


def gate(name, condition, evidence=None):
    REPORT['gates'][name] = {'status': 'Satisfied' if condition else 'Violated', 'evidence': evidence}
    if not condition:
        raise AssertionError(name)


def request(method, path, token=None, body=None):
    status, value = owner.http(method, path, token, body)
    if isinstance(value, bytes):
        value = {'raw_response_sha256': hashlib.sha256(value).hexdigest(), 'bytes': len(value)}
    return status, value


def config_update(update):
    path = STATE / 'config.json'
    config = json.loads(path.read_text())
    update(config)
    temporary = path.with_suffix('.index-next')
    temporary.write_text(json.dumps(config, indent=2) + '\n')
    temporary.chmod(0o600)
    temporary.replace(path)


def restart():
    env = dict(os.environ, SYSML_INDEXING_ENABLED='false')
    owner.run(['bash', str(ROOT / 'scripts/sysml-owner-runtime.sh'), 'owner-restart'], timeout=60, env=env)


def kill_restart(label):
    # Real process termination; no production request or environment kill hook.
    owner.run(['podman', 'kill', '--signal', 'KILL', POD + '-owner'])
    inspect = json.loads(owner.run(['podman', 'inspect', POD + '-owner']).stdout)[0]
    gate(label + '-terminated', not inspect['State']['Running'], {'exit': inspect['State'].get('ExitCode')})
    restart()


def main():
    # Authoritative handles are inspected before any restart or mutation.
    handles = json.loads(owner.run(['podman', 'inspect', POD + '-owner', POD + '-native', POD + '-db']).stdout)
    REPORT['runtime_before'] = [{'id': h['Id'], 'name': h['Name'], 'running': h['State']['Running']} for h in handles]
    if not all(h['State']['Running'] for h in handles):
        raise owner.Unknown('existing private runtime is not running; use sysml-owner-up explicitly')
    config = json.loads((STATE / 'config.json').read_text())
    tokens = {c['actor']: c['token'] for c in config['credentials']}
    project, branch = 'live-project', 'main'
    remote = next(p for p in config['projects'] if p['project'] == project)
    base = f'/v1/projects/{project}/branches/{branch}'
    run_id = 'index-' + str(time.time_ns())
    fixture = ROOT / 'crates/ledgrrr-sysml-adapter/tests/fixtures/native_revision_cases.json'
    REPORT['fixture_sha256'] = hashlib.sha256(fixture.read_bytes()).hexdigest()
    REPORT['fixture_context'] = json.loads(fixture.read_text())['context']
    REPORT['pins'] = json.loads((STATE / 'pins.json').read_text())
    REPORT['canonical_contract'] = [p for p in tomllib.loads((ROOT / 'Cargo.lock').read_text())['package'] if p['name'] == 'ufo-types']
    REPORT['implementation_commit'] = owner.run(['git', '-C', str(ROOT), 'rev-parse', 'HEAD']).stdout.decode().strip()
    source_files = sorted([ROOT / 'Cargo.toml', ROOT / 'Cargo.lock'] +
        [p for directory in ('crates/ledgrrr-revision-io', 'crates/ledgrrr-sysml-adapter')
         for p in (ROOT / directory).rglob('*') if p.is_file()])
    REPORT['implementation_source_files'] = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
                                              for p in source_files}
    binary = Path(os.environ.get('SYSML_OWNER_BINARY', '/tmp/sysml-implementation/ledgrrr-target/debug/revision-owner'))
    REPORT['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()

    def count():
        # Native commits accumulate across runs; a full page is not a complete count.
        path = f"/projects/{remote['remote_project']}/commits"
        current = 'http://127.0.0.1:9000' + path
        visited, identities = set(), set()
        for _ in range(100):
            if current in visited:
                raise owner.Unknown('native commit observer repeated pagination URL')
            visited.add(current)
            result = owner.run(['podman', 'exec', POD + '-native', 'curl', '-sS', '-f', '--max-time', '20', '-i', current])
            if len(result.stdout) > 8 * 1024 * 1024:
                raise owner.Unknown('native commit observer response exceeded bound')
            headers, body = result.stdout.split(b'\r\n\r\n', 1)
            page = json.loads(body)
            if not isinstance(page, list):
                raise owner.Unknown('native commit observer expected a list')
            for commit in page:
                identity = commit['@id']
                if identity in identities:
                    raise owner.Unknown('native commit observer repeated commit identity')
                identities.add(identity)
            links = [line.split(b':', 1)[1].decode() for line in headers.split(b'\r\n')
                     if line.lower().startswith(b'link:')]
            next_links = [match.group(1) for link in links
                          for match in re.finditer(r'<([^>]+)>\s*;\s*rel="?next"?', link)]
            if not next_links:
                return len(identities)
            if len(next_links) != 1:
                raise owner.Unknown('native commit observer ambiguous next page')
            current = urllib.parse.urljoin(current, next_links[0])
            parsed = urllib.parse.urlsplit(current)
            if parsed.scheme != 'http' or parsed.netloc != '127.0.0.1:9000' or parsed.path != path or parsed.fragment:
                raise owner.Unknown('native commit observer pagination escaped configured route')
        raise owner.Unknown('native commit observer exceeded page bound')

    def head():
        status, value = request('GET', base + '/head', tokens['reader'])
        gate('head-readable-' + str(time.time_ns()), status == 200, value)
        return value['model_revision']

    def checkpoint(revision):
        return request('GET', f'/v1/projects/{project}/revisions/{revision}/checkpoint', tokens['reader'])

    # Normal semantic gates use the service's two-second budget. The private
    # provider head read itself takes ~300ms; 250ms tests only head timeout.
    def query(selector, text='ASK {}', deadline=2000, token=None, overrides=None):
        body = {'project': project, 'branch': branch, 'selector': selector, 'query': text, 'deadline_ms': deadline}
        body.update(overrides or {})
        return request('POST', base + '/query', token or tokens['reader'], body)

    def exact(revision, text='ASK {}', deadline=2000):
        return query({'kind': 'exact', 'revision': revision}, text, deadline)

    def completed(label, response, revision):
        status, value = response
        outcome = value.get('outcome', {}) if isinstance(value, dict) else {}
        gate(label, status == 200 and outcome.get('kind') == 'completed'
             and outcome.get('indexed_revision') == revision
             and outcome.get('graph', {}).get('checkpoint', {}).get('revision') == revision, value)
        REPORT['observations'][label] = value
        return outcome

    def cli(action, revision, token_file=None):
        args = ['podman', 'exec', POD + '-owner', '/owner/revision-owner', 'index', '--config', '/owner/config.json',
                '--project', project, '--revision', revision, '--action', action]
        if token_file:
            args += ['--token-file', '/owner/' + token_file]
        result = owner.run(args, timeout=60)
        try:
            value = json.loads(result.stdout)
        except ValueError:
            value = {'stdout': result.stdout.decode(errors='replace')[-2000:]}
        REPORT['observations'][f'cli-{action}-{revision}-{time.time_ns()}'] = value
        return value

    def accept(case, parent, candidate=None, suffix=None, actor='editor-a'):
        candidate = candidate if candidate is not None else owner.bundle(project, case, parent)
        operation = run_id + '-' + (suffix or case)
        expected = {'kind': 'revision', 'revision': parent} if parent else {'kind': 'empty'}
        path = base + '/operations/' + operation
        status, value = request('POST', path, tokens[actor], {'expected_head': expected, 'bundle': candidate})
        gate(operation + '-intake', status == 200, value)
        status, value = request('POST', path + '/promote', tokens[actor])
        revision = owner.revision(value)
        gate(operation + '-accepted', status == 200 and bool(revision), value)
        REPORT['observations'][operation] = value
        return revision, value, candidate

    # Stop background indexing deterministically; never stop or reset provider.
    restart()
    initial = head()
    r0, receipt, submitted = accept('base', initial)
    count_accepted = count()
    status, pending = exact(r0)
    gate('receipt-before-index-is-pending', status == 200 and pending['outcome']['kind'] == 'pending', pending)
    token_file = run_id + '-claim.json'
    cli('claim', r0, token_file)
    kill_restart('after-claim')
    cli('seal', r0, token_file)
    status, unpublished = checkpoint(r0)
    gate('sealed-graph-remains-unpublished', status == 404 or
         (status == 200 and isinstance(unpublished, dict) and unpublished.get('outcome', {}).get('kind') == 'pending'),
         {'status': status, 'response': unpublished})
    status, sealed_pending = exact(r0)
    gate('sealed-query-cannot-expose-partial-graph', status == 200
         and sealed_pending.get('outcome', {}).get('kind') == 'pending', sealed_pending)
    kill_restart('after-seal-before-publication')
    cli('publish', r0, token_file)
    first = completed('exact-first-revision', exact(r0), r0)
    descriptor0 = first['graph']
    gate('published-actual-candidate', descriptor0['accepted_candidate_digest'] == receipt['prepared']['candidate_digest'], descriptor0)
    missing = completed('exact-unverified-requirement-query', exact(r0, UNVERIFIED), r0)
    rows = missing['results']['rows']
    gate('known-unverified-requirement', any(row.get('id', {}).get('value') == 'REQ-2' for row in rows), rows)
    paths = completed('exact-code-to-requirement-query', exact(r0, SOURCE_PATH), r0)['results']['rows']
    gate('source-path-retains-authority-and-coordinate', any(row.get('id', {}).get('value') == 'REQ-日本語'
         and row.get('file', {}).get('value') == 'src/controller.rs'
         and row.get('line', {}).get('value') == '1'
         and row.get('authority', {}).get('value') == 'authored' for row in paths), paths)
    status, exported = request('GET', f'/v1/projects/{project}/revisions/{r0}/bundle', tokens['reader'])
    gate('query-export-share-accepted-model-and-bytes', status == 200
         and exported['model'] == submitted['model'] and exported['blobs'] == submitted['blobs'],
         {'status': status, 'source_blob_digests': list(submitted['blobs'])})
    gate('crash-replay-no-native-commit', count() == count_accepted, {'before': count_accepted, 'after': count()})
    status, checkpoint0 = checkpoint(r0)
    gate('checkpoint-matches-query-manifest', status == 200 and checkpoint0 == descriptor0, checkpoint0)
    status, stored = request('GET', f"/v1/projects/{project}/operations/{receipt['receipt']['operation']}", tokens['reader'])
    gate('receipt-indexed-only-after-publication', status == 200 and stored['receipt']['status']['kind'] == 'indexed', stored)
    # Re-importing the complete accepted envelope observes the same model commit.
    before_noop = count()
    noop_parent = exported['manifest']['context'].get('parent')
    rn, noop, _ = accept('base', noop_parent, candidate=exported, suffix='observed-noop')
    cli('run', rn)
    noop_graph = completed('noop-shares-revision-checkpoint', exact(rn), rn)['graph']
    gate('noop-deduplicates-without-native-create', rn == r0 and count() == before_noop
         and noop_graph == descriptor0 and noop.get('observed_existing') is True, noop)

    # Preserve older exact queries while jobs finish in reverse order.
    ra, a, _ = accept('edit_a', r0, suffix='ordering-a')
    stale = completed('explicit-older-query-during-pending-index', query({'kind': 'current', 'allow_older': True}), r0)
    gate('older-result-declares-pending-model-head', stale['freshness'] == 'stale' and stale['model_revision'] == ra, stale)
    started = time.monotonic()
    status, waiting = query({'kind': 'current', 'allow_older': False}, deadline=2000)
    wait_elapsed = time.monotonic() - started
    gate('read-your-write-deadline-remains-pending', status == 200 and waiting['outcome']['kind'] == 'pending'
         and wait_elapsed < 5, {'elapsed_seconds': wait_elapsed, 'response': waiting})
    rb, b, _ = accept('edit_b', ra, suffix='ordering-b')
    older_token = run_id + '-older.json'
    cli('claim', ra, older_token)
    cli('seal', ra, older_token)
    cli('run', rb)
    newer = completed('newer-completes-first', exact(rb), rb)
    cli('publish', ra, older_token)
    older = completed('older-still-exactly-queryable', exact(ra), ra)
    gate('historical-answer-explicitly-stale', older['freshness'] == 'stale' and older['model_revision'] == rb, older)
    completed('old-job-does-not-rewind-current', query({'kind': 'current', 'allow_older': False}), rb)
    completed('minimum-checkpoint-uses-proven-lineage', query({'kind': 'minimum', 'checkpoint': descriptor0['checkpoint'], 'allow_older': False}), rb)
    forged = dict(descriptor0['checkpoint'], graph_digest='sha256:' + '0' * 64)
    invalid_minimum = query({'kind': 'minimum', 'checkpoint': forged, 'allow_older': False})
    gate('minimum-checkpoint-digest-is-authoritative', invalid_minimum[0] in (400, 422)
         or invalid_minimum[1].get('outcome', {}).get('kind') == 'unavailable', invalid_minimum)

    # A stale proposal rebases: graph input must be its accepted candidate.
    # r0 is the original base: edit_a changes a different requirement from
    # the current edit_b head. Using ra would submit no semantic edit at all.
    rm, merged, _ = accept('edit_a', r0, suffix='rebased', actor='editor-b')
    cli('run', rm)
    merged_graph = completed('merged-accepted-graph', exact(rm), rm)['graph']
    gate('merged-graph-not-original-proposal', merged_graph['accepted_candidate_digest'] == merged['prepared']['candidate_digest']
         and merged_graph['accepted_candidate_digest'] != merged['receipt']['proposal_digest'], merged)

    status, merged_export = request('GET', f'/v1/projects/{project}/revisions/{rm}/bundle', tokens['reader'])
    gate('rebased-candidate-retains-both-disjoint-edits', status == 200
         and merged_export['model']['elements']['REQ-日本語']['name'] == 'Editor A actuator'
         and merged_export['model']['elements']['REQ-2']['name'] == 'Editor B idle',
         {'status': status, 'model': merged_export.get('model')})

    # Accepted manifests survive projection loss and reproduce exact graph bytes.
    model_count = count()
    digest = merged_graph['checkpoint']['graph_digest']
    cli('remove-projection', rm)
    status, lost = exact(rm)
    gate('projection-loss-unavailable', status == 200 and lost['outcome']['kind'] == 'unavailable', lost)
    cli('rebuild', rm)
    rebuilt = completed('deterministic-rebuild', exact(rm), rm)
    gate('rebuild-identical-digest', rebuilt['graph']['checkpoint']['graph_digest'] == digest, rebuilt['graph'])
    gate('rebuild-never-mutates-native-model', count() == model_count, {'before': model_count, 'after': count()})

    # Every query remains authorized independently of known artifact/checkpoint IDs.
    unauth = owner.http('POST', base + '/query', None, {'project': project, 'branch': branch,
        'selector': {'kind': 'exact', 'revision': rm}, 'query': 'ASK {}', 'deadline_ms': 100})
    gate('query-authentication-required', unauth[0] in (401, 403), {'status': unauth[0]})
    mismatch = query({'kind': 'exact', 'revision': rm}, overrides={'project': 'fresh-project'})
    gate('query-path-body-project-mismatch-refused', mismatch[0] in (400, 403, 422), {'status': mismatch[0]})
    foreign_status, foreign = request('POST', '/v1/projects/fresh-project/branches/main/query', tokens['reader'],
        {'project': 'fresh-project', 'branch': branch, 'selector': {'kind': 'exact', 'revision': rm},
         'query': 'ASK {}', 'deadline_ms': 100})
    gate('cross-project-revision-does-not-expose-graph', foreign_status in (403, 404) or
         (foreign_status == 200 and foreign.get('outcome', {}).get('kind') == 'unavailable'
          and 'graph' not in foreign.get('outcome', {})), foreign)
    forbidden = exact(rm, 'SELECT * WHERE { FILTER EXISTS { SERVICE <http://127.0.0.1:9/> { ?s ?p ?o } } }')
    gate('recursive-service-refused', forbidden[0] in (400, 422) or
         (forbidden[1].get('outcome', {}).get('kind') == 'unavailable'
          and forbidden[1]['outcome'].get('reason') in ('invalid_query', 'unsupported_query')), forbidden)

    # Bound an actual evaluator before its first aggregate row, then prove capacity returns.
    values = ' '.join(str(n) for n in range(80))
    expensive = 'SELECT (COUNT(*) AS ?n) WHERE { ' + ' '.join(
        f'VALUES ?v{i} {{ {values} }}' for i in range(8)) + ' }'
    def owner_processes():
        result = owner.run(['podman', 'top', POD + '-owner', 'pid,args'])
        return {line.split(None, 1)[0]: line.split(None, 1)[1]
                for line in result.stdout.decode().splitlines()[1:]
                if len(line.split(None, 1)) == 2}

    def query_workers():
        return {pid: command for pid, command in owner_processes().items() if 'query-worker' in command}

    gate('no-worker-before-expensive-request', not query_workers())
    started = time.monotonic()
    observed = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
        pending_query = executor.submit(exact, rm, expensive, 2000)
        while not pending_query.done():
            observed.update(query_workers())
            time.sleep(0.025)
        timeout = pending_query.result()
    elapsed = time.monotonic() - started
    gate('expensive-query-reached-evaluator-process', bool(observed), observed)
    gate('bounded-expensive-evaluation', elapsed < 5 and timeout[0] == 200
         and timeout[1].get('outcome', {}).get('reason') == 'deadline_exceeded', {'elapsed_seconds': elapsed, 'response': timeout})
    remaining = owner_processes()
    gate('timed-out-query-worker-is-reaped', set(observed).isdisjoint(remaining)
         and not any('query-worker' in command for command in remaining.values()),
         {'observed_worker_pids': sorted(observed), 'remaining_processes': remaining})
    completed('capacity-after-evaluator-cancellation', exact(rm, deadline=2000), rm)

    # Grant revocation must invalidate access even after the graph was queried.
    config_update(lambda c: [a.update(read=False) for p in c['projects'] if p['project'] == project
                            for a in p['actors'] if a['actor'] == 'reader'])
    restart()
    denied = exact(rm)
    gate('revoked-grant-denies-cached-graph', denied[0] == 403, {'status': denied[0]})
    config_update(lambda c: [a.update(read=True) for p in c['projects'] if p['project'] == project
                            for a in p['actors'] if a['actor'] == 'reader'])
    restart()

    empty, _, _ = accept('empty', rm, suffix='deleted-all')
    cli('run', empty)
    empty_graph = completed('deleted-all-complete-revision', exact(empty), empty)
    gate('empty-model-still-has-revision-metadata', empty_graph['graph']['quad_count'] > 0, empty_graph['graph'])
    deleted_rows = completed('deleted-all-has-no-old-requirements', exact(empty, UNVERIFIED), empty)['results']['rows']
    gate('tombstones-remove-prior-facts', deleted_rows == [], deleted_rows)
    completed('historical-graph-survives-deletion', exact(r0), r0)
    owner.run(['bash', str(ROOT / 'scripts/sysml-owner-runtime.sh'), 'owner-restart'],
              timeout=60, env=dict(os.environ, SYSML_INDEXING_ENABLED='true'))
    background_revision, _, _ = accept('base', empty, suffix='background-worker')
    expires = time.monotonic() + 20
    last = None
    while time.monotonic() < expires:
        last = exact(background_revision, deadline=2000)
        if last[0] == 200 and last[1].get('outcome', {}).get('kind') == 'completed':
            break
        time.sleep(0.1)
    completed('reachable-background-worker-publishes-accepted-revision', last, background_revision)
    REPORT['status'] = 'Satisfied'


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    exit_code = 0
    try:
        main()
    except AssertionError as error:
        REPORT['status'], REPORT['error'], exit_code = 'Violated', str(error), 1
    except (owner.Unknown, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as error:
        REPORT['status'], REPORT['error'], exit_code = 'Unknown', str(error), 2
    finally:
        # Restore read grant and background policy after any partial failure.
        # Restoration does not manufacture verification gates or retry publication.
        try:
            if (STATE / 'config.json').exists():
                config_update(lambda c: [a.update(read=True) for p in c['projects'] if p['project'] == 'live-project'
                                        for a in p['actors'] if a['actor'] == 'reader'])
                owner.run(['bash', str(ROOT / 'scripts/sysml-owner-runtime.sh'), 'owner-restart'],
                          timeout=60, env=dict(os.environ, SYSML_INDEXING_ENABLED='true'))
        except (owner.Unknown, OSError, ValueError, subprocess.TimeoutExpired) as error:
            REPORT['restoration_error'] = str(error)
            if exit_code == 0:
                REPORT['status'], exit_code = 'Unknown', 2
        try:
            handles = json.loads(owner.run(['podman', 'inspect', POD + '-owner', POD + '-native', POD + '-db']).stdout)
            REPORT['runtime_after'] = [{'id': h['Id'], 'name': h['Name'],
                'running': h['State']['Running'], 'exit_code': h['State'].get('ExitCode')} for h in handles]
            if not all(h['State']['Running'] for h in handles) and exit_code == 0:
                REPORT['status'], exit_code = 'Unknown', 2
        except (owner.Unknown, OSError, ValueError, subprocess.TimeoutExpired) as error:
            REPORT['runtime_observation_error'] = str(error)
            if exit_code == 0:
                REPORT['status'], exit_code = 'Unknown', 2
        REPORT['terminal_exit_code'] = exit_code
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(REPORT, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({'status': REPORT['status'], 'gates': len(REPORT['gates']), 'report': str(args.output)}))
    sys.exit(exit_code)
