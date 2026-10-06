#!/usr/bin/env python3
"""Real authenticated HTTP evidence. Unknown infrastructure is never success."""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

STATE = Path(os.environ.get('SYSML_OWNER_STATE', '/tmp/sysml-owner-c9d'))
POD = 'pex-sysml-owner-c9d'
JAVA = 'docker.io/library/eclipse-temurin@sha256:8db2bcf62ae171d1247c331db0410d8bc6347e7493bdafe104bb9a33d59c1291'
ROOT = Path(__file__).resolve().parent.parent
REPORT = {'status': 'Unknown', 'gates': {}, 'observations': {}, 'limitations': [
    'No graph publication/query, browser/UI, resolved derivation-library or general behavior claim.',
    'Podman/host/database administrators are trusted; independent owner databases are unsupported.']}

class Unknown(Exception):
    pass

def run(args, timeout=60, check=True, env=None):
    r = subprocess.run(args, capture_output=True, timeout=timeout, env=env)
    if check and r.returncode:
        raise Unknown(f'{args[0]} failed ({r.returncode}): {r.stderr.decode(errors="replace")[-400:]}')
    return r

def gate(name, passed, details=None):
    REPORT['gates'][name] = {'status': 'Satisfied' if passed else 'Violated', 'details': details}
    if not passed:
        raise AssertionError(name)

def http(method, path, token=None, body=None):
    headers = {'Content-Type': 'application/json'}
    if token:
        headers['Authorization'] = 'Bearer ' + token
    data = None if body is None else json.dumps(body, ensure_ascii=False).encode()
    req = urllib.request.Request('http://127.0.0.1:19001' + path, data=data, headers=headers, method=method)
    try:
        response = urllib.request.urlopen(req, timeout=35)
    except urllib.error.HTTPError as e:
        response = e
    except (urllib.error.URLError, OSError, TimeoutError) as e:
        raise Unknown(f'owner unavailable: {e}') from e
    with response:
        raw = response.read(8 * 1024 * 1024 + 1)
        if len(raw) > 8 * 1024 * 1024:
            raise Unknown('owner response exceeded evidence bound')
        try:
            value = json.loads(raw) if 'json' in response.headers.get('Content-Type','') else raw
        except (ValueError, UnicodeError):
            value = raw
        return response.code, value

def native(path):
    # Read-only privileged observer; ordinary clients never receive this access.
    r = run(['podman', 'exec', POD + '-native', 'curl', '-sS', '-f', '--max-time', '20', '-i',
             'http://127.0.0.1:9000' + path])
    headers, body = r.stdout.split(b'\r\n\r\n', 1)
    if b'rel="next"' in headers or b'rel=next' in headers:
        raise Unknown('native observer pagination required: cannot claim complete evidence')
    return json.loads(body)

def revision(operation):
    r = operation.get('actual_revision')
    if r:
        return r
    status = operation.get('receipt', {}).get('status', {})
    if isinstance(status, dict):
        return status.get('revision') or status.get('model_revision')
    return None

def bundle(project, case, parent, existing=None):
    path = STATE / f'fixture-{project}-{case}-{time.time_ns()}.json'
    binary = os.environ.get('SYSML_OWNER_BINARY', '/tmp/sysml-implementation/ledgrrr-target/debug/revision-owner')
    args = [binary, 'fixture', '--project', project, '--case', case, '--output', str(path)]
    if parent:
        args += ['--parent', parent]
    if existing is not None:
        source=STATE/f'fixture-input-{time.time_ns()}.json'
        source.write_text(json.dumps(existing,ensure_ascii=False))
        args += ['--input',str(source)]
    run(args)
    return json.loads(path.read_text())

def main():
    config = json.loads((STATE / 'config.json').read_text())
    tokens = {x['actor']: x['token'] for x in config['credentials']}
    project = 'live-project'; branch = 'main'; remote = next(x for x in config['projects'] if x['project'] == project)
    p = remote['remote_project']; b = remote['remote_branch']
    prefix = f'/v1/projects/{project}/branches/{branch}/operations/'
    REPORT['pins'] = json.loads((STATE / 'pins.json').read_text())
    fixture = ROOT / 'crates/ledgrrr-sysml-adapter/tests/fixtures/native_revision_cases.json'
    REPORT['fixture_sha256'] = hashlib.sha256(fixture.read_bytes()).hexdigest()
    fixture_data=json.loads(fixture.read_text())
    REPORT['fixture_source_library_toolchain']=fixture_data['context']
    operations_fixture=fixture.parent/'owner_operation_cases.json'
    REPORT['operation_fixture_sha256']=hashlib.sha256(operations_fixture.read_bytes()).hexdigest()
    lock=tomllib.loads((ROOT/'Cargo.lock').read_text())
    REPORT['canonical_contract_pins']=[{'name':x['name'],'version':x['version'],'source':x.get('source')} for x in lock['package'] if x['name']=='ufo-types']
    REPORT['native_library_resolution']='Unsupported: fixture library/toolchain labels are retained evidence, not native resolved identities'
    REPORT['owner_binary_sha256']=hashlib.sha256((STATE/'revision-owner').read_bytes()).hexdigest()
    gate('owner_ready', http('GET', f'/v1/projects/{project}/branches/{branch}/head', tokens['reader'])[0] == 200)
    gate('unauthenticated_denied', http('GET', f'/v1/projects/{project}/branches/{branch}/head')[0] in (401, 403))
    alternate=dict(config); alternate['store_path']='/owner/alternate.sqlite'
    alternate_path=STATE/'alternate-config.json'
    alternate_path.write_text(json.dumps(alternate)); alternate_path.chmod(0o600)
    duplicate=run(['podman','exec',POD+'-owner','/owner/revision-owner','--config','/owner/alternate-config.json'],check=False)
    gate('independent_owner_database_refused',duplicate.returncode!=0 and
         b'already bound to another owner database' in duplicate.stderr and not (STATE/'alternate.sqlite').exists(),
         {'exit':duplicate.returncode,'authority_guard_refused':b'already bound to another owner database' in duplicate.stderr})
    # Enumerate every native mutation from pinned official routes plus alternate prefixes.
    routes = Path(os.environ.get('SYSML_REFERENCE_SOURCE', '/home/brianh/promptexecution/.worktrees/sysml-v2-api-reference')) / 'conf/routes'
    inventory = []
    for row in routes.read_text().splitlines():
        words = row.split()
        if len(words) >= 2 and words[0] in ('POST', 'PUT', 'DELETE'):
            path = words[1]
            for name in ('projectId', 'branchId', 'tagId', 'queryId', 'commitId'):
                path = path.replace(':' + name, '00000000-0000-0000-0000-000000000000')
            for alt in ('', '/api', '/v1'):
                status, _ = http(words[0], alt + path, tokens['owner'], {})
                inventory.append({'method': words[0], 'path': alt + path, 'status': status})
    gate('all_native_mutation_routes_denied', all(x['status'] in (400, 401, 403, 404, 405) for x in inventory), inventory)
    inspect = json.loads(run(['podman', 'pod', 'inspect', POD]).stdout)
    inspect = inspect[0] if isinstance(inspect, list) else inspect
    bindings = inspect['InfraConfig']['PortBindings']
    gate('only_owner_published', bindings == {'19001/tcp': [{'HostIp': '127.0.0.1', 'HostPort': '19001'}]}, bindings)
    infra = inspect['InfraContainerID']
    info = json.loads(run(['podman', 'inspect', infra]).stdout)[0]
    addresses = [x['IPAddress'] for x in info['NetworkSettings'].get('Networks', {}).values() if x.get('IPAddress')]
    if not addresses:
        # Rootless pasta copies interface addresses into the isolated namespace
        # and does not populate Podman bridge-IP metadata.
        addresses=run(['podman','exec',POD+'-native','hostname','-I']).stdout.decode().split()
    sockets=run(['podman','exec',POD+'-native','cat','/proc/net/tcp','/proc/net/tcp6']).stdout.decode().splitlines()
    native_listeners=[row.split()[1] for row in sockets[1:] if len(row.split())>3 and row.split()[3]=='0A' and row.split()[1].endswith(':2328')]
    gate('native_socket_is_pod_loopback',bool(native_listeners) and all(x in ('0100007F:2328','0000000000000000FFFF00000100007F:2328','00000000000000000000000001000000:2328') for x in native_listeners),native_listeners)
    gate('network_address_observed', bool(addresses), addresses)
    isolation = []
    for address in addresses:
        target = f'http://[{address}]:9000/projects' if ':' in address else f'http://{address}:9000/projects'
        host = run(['curl', '-sS', '--noproxy', '*', '--connect-timeout', '2', '--max-time', '3', target], check=False)
        outsider = run(['podman', 'run', '--rm', '--name', POD + '-isolation-probe', JAVA, 'curl', '-sS',
                        '--noproxy', '*', '--connect-timeout', '2', '--max-time', '3', target], check=False)
        isolation.append({'address': address, 'host_exit': host.returncode, 'unrelated_container_exit': outsider.returncode})
    gate('backend_network_isolated', all(x['host_exit'] != 0 and x['unrelated_container_exit'] != 0 for x in isolation), isolation)
    initial = native(f'/projects/{p}/branches/{b}').get('head')
    initial = initial.get('@id') if isinstance(initial, dict) else initial
    before = native(f'/projects/{p}/commits')
    run_id = str(time.time_ns())

    operation_actors={}
    def intake(case, parent, actor='editor-a', candidate=None, suffix=None):
        op = run_id + '-' + (suffix or case)
        candidate = candidate or bundle(project, case, parent)
        expected = {'kind': 'revision', 'revision': parent} if parent else {'kind': 'empty'}
        status, value = http('POST', prefix + op, tokens[actor], {'expected_head': expected, 'bundle': candidate})
        gate('intake-' + op, status in (200, 201, 202), {'status': status, 'response': value})
        operation_actors[op]=actor
        return op, candidate

    def promote(op):
        status, value = http('POST', prefix + op + '/promote', tokens[operation_actors[op]])
        REPORT['observations'][op] = {'status': status, 'operation': value}
        return status, value

    unsupported=bundle(project,'unsupported_action',initial)
    bad_op,_=intake('unsupported_action',initial,candidate=unsupported,suffix='unsupported')
    count_before=len(native(f'/projects/{p}/commits'))
    bad_status,bad_result=promote(bad_op)
    gate('unsupported_native_refused_before_create',bad_status==422 and len(native(f'/projects/{p}/commits'))==count_before,bad_result)
    first, original = intake('base', initial)
    # Spoofing an actor is rejected before intake rather than selecting another grant.
    spoof, _ = http('POST', prefix + run_id + '-spoof', tokens['editor-a'], {'actor': 'owner',
        'expected_head': {'kind': 'revision', 'revision': initial} if initial else {'kind': 'empty'}, 'bundle': original})
    gate('spoofed_actor_denied', spoof in (400, 403, 422))
    denied, _ = http('POST', prefix + run_id + '-reader', tokens['reader'], {'expected_head':
        {'kind': 'revision', 'revision': initial} if initial else {'kind': 'empty'}, 'bundle': original})
    gate('reader_cannot_intake', denied in (401, 403))
    denied, _ = http('POST', prefix + first + '/promote', tokens['reader'])
    gate('reader_cannot_promote', denied in (401, 403))
    status, first_result = promote(first)
    r0 = revision(first_result)
    REPORT['materialized_base_bundle_sha256']=hashlib.sha256(json.dumps(original,ensure_ascii=False).encode()).hexdigest()
    raw_digest=first_result.get('raw_envelope')
    if not raw_digest: raise Unknown('original submitted envelope artifact absent')
    raw_status,raw_bytes=http('GET',f'/v1/projects/{project}/artifacts/{raw_digest}',tokens['reader'])
    gate('original_submitted_envelope_bytes',raw_status==200 and raw_bytes==json.dumps(original,ensure_ascii=False).encode())
    gate('first_actual_commit', status == 200 and bool(r0), first_result)
    exact = native(f'/projects/{p}/commits/{r0}')
    elements = native(f'/projects/{p}/commits/{r0}/elements?excludeUsed=false')
    gate('native_parent_observed', (exact.get('previousCommit') or {}).get('@id') == initial, exact)
    code, fetched = http('GET', f'/v1/projects/{project}/revisions/{r0}/bundle', tokens['reader'])
    gate('fetch_emit_fetch_semantics', code == 200 and fetched.get('model') == original.get('model'), {'native': elements, 'bundle': fetched})
    gate('original_blob_bytes', bool(original.get('blobs')) and fetched.get('blobs') == original.get('blobs'))
    blob_results=[]
    for digest, content in original.get('blobs',{}).items():
        code, actual=http('GET',f'/v1/projects/{project}/artifacts/{digest}',tokens['reader'])
        raw=bytes(actual) if isinstance(actual,list) else actual
        blob_results.append({'digest':digest,'matches':code==200 and raw==bytes(content)})
        denied,_=http('GET',f'/v1/projects/{project}/artifacts/{digest}')
        gate('unauthorized-artifact-'+digest,denied in (401,403))
    gate('artifact_export_bytes',bool(blob_results) and all(x['matches'] for x in blob_results),blob_results)
    # Identical original envelope is a no-op even though its recorded proposal parent is old.
    noop, _ = intake('base', initial, candidate=original, suffix='noop')
    count0 = len(native(f'/projects/{p}/commits'))
    ns, nr = promote(noop)
    count1 = len(native(f'/projects/{p}/commits'))
    gate('full_envelope_noop', ns == 200 and revision(nr) == r0 and count0 == count1, {'before':count0,'after':count1,'receipt':nr})
    changed, changed_bundle=intake('base',r0,suffix='equal-semantics-changed-context')
    count0=len(native(f'/projects/{p}/commits'))
    change_status,change_result=promote(changed)
    gate('changed_envelope_is_not_noop',change_status==200 and revision(change_result)!=r0 and
         len(native(f'/projects/{p}/commits'))==count0+1 and
         changed_bundle['manifest']['semantic_digest']==original['manifest']['semantic_digest'],change_result)
    r0=revision(change_result)
    a, edit_a = intake('edit_a', r0, 'editor-a')
    bb, edit_b = intake('edit_b', r0, 'editor-b')
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(promote, [a, bb]))
    if any(x[0] in (409, 423, 503) and not revision(x[1]) for x in results):
        # Reservations may reject concurrent acquisition; retry only a never-dispatched intake.
        results = [promote(op) if not revision(result) else (code,result) for op,(code,result) in zip([a,bb],results)]
    gate('disjoint_merge_accepted', all(code == 200 and revision(result) for code,result in results), results)
    head = native(f'/projects/{p}/branches/{b}')['head']['@id']
    _, merged = http('GET', f'/v1/projects/{project}/revisions/{head}/bundle', tokens['reader'])
    names = [x.get('name') for x in merged.get('model',{}).get('elements',{}).values()]
    REPORT['observations']['merged_bundle'] = merged
    expected_elements=dict(original['model']['elements'])
    for candidate in (edit_a,edit_b):
        for key,value in candidate['model']['elements'].items():
            if value != original['model']['elements'].get(key): expected_elements[key]=value
    gate('two_edits_retained', len(set(revision(result) for _,result in results)) == 2 and
         merged['model']['elements']==expected_elements, {'expected':expected_elements,'actual':merged['model']['elements']})
    ca, _ = intake('conflict_a', head)
    cb, _ = intake('conflict_b', head, 'editor-b')
    promote(ca)
    oldcount = len(native(f'/projects/{p}/commits'))
    cs, cr = promote(cb)
    conflict_status=cr.get('receipt',{}).get('status',{})
    gate('typed_conflict_without_commit', cs in (200,409) and isinstance(conflict_status,dict) and
         conflict_status.get('kind')=='conflict' and len(native(f'/projects/{p}/commits'))==oldcount,cr)
    conflict_digest=cr.get('conflict_evidence')
    if not conflict_digest: raise Unknown('typed upstream conflict artifact absent: rebuild latest owner')
    code,conflict_bytes=http('GET',f'/v1/projects/{project}/artifacts/{conflict_digest}',tokens['reader'])
    conflict_rows=json.loads(conflict_bytes) if isinstance(conflict_bytes,bytes) else conflict_bytes
    gate('upstream_typed_conflict_artifact',code==200 and isinstance(conflict_rows,list) and bool(conflict_rows),conflict_rows)
    # Native preserving-schema restart independently checks exact prior revision bytes.
    snapshot = native(f'/projects/{p}/commits/{r0}/elements?excludeUsed=false')
    run(['bash', str(ROOT/'scripts/sysml-owner-runtime.sh'), 'native-restart'], timeout=150)
    gate('native_restart_preserves_exact_revision', native(f'/projects/{p}/commits/{r0}/elements?excludeUsed=false') == snapshot)
    run(['bash', str(ROOT/'scripts/sysml-owner-runtime.sh'), 'owner-restart'], timeout=100)
    code, recovered = http('GET', f'/v1/projects/{project}/operations/{first}', tokens['reader'])
    gate('owner_restart_preserves_receipt', code == 200 and revision(recovered) == revision(first_result), recovered)
    REPORT['observations']['initial_commit_count'] = len(before)
    REPORT['observations']['final_commit_count'] = len(native(f'/projects/{p}/commits'))
    # Independently exercise a subprocess exit after remote acceptance and an
    # explicitly ambiguous response. Neither can authorize a duplicate send.
    for mode in ('CRASH','AMBIGUOUS'):
        current_head=native(f'/projects/{p}/branches/{b}')['head']['@id']
        code,current_bundle=http('GET',f'/v1/projects/{project}/revisions/{current_head}/bundle',tokens['reader'])
        gate('pre-crash-fetch-'+mode,code==200)
        candidate=bundle(project,'edit_a',current_head,existing=current_bundle)
        op,_=intake('edit_a',current_head,candidate=candidate,suffix=mode.lower())
        oldcount=len(native(f'/projects/{p}/commits'))
        env=dict(os.environ)
        env.pop('OWNER_TEST_CRASH_AFTER_CREATE',None); env.pop('OWNER_TEST_AMBIGUOUS_AFTER_CREATE',None)
        env['OWNER_TEST_'+mode+'_AFTER_CREATE']='1'
        run(['bash',str(ROOT/'scripts/sysml-owner-runtime.sh'),'owner-restart'],timeout=100,env=env)
        try:
            code,value=promote(op)
            if mode=='CRASH': raise AssertionError('crash hook did not interrupt response')
            REPORT['observations']['ambiguous_response']=value
            gate('ambiguous-not-committed',not revision(value) and value.get('receipt',{}).get('status',{}).get('kind')=='ambiguous',value)
            code,blocked=promote(op)
            gate('ambiguous-retry-remains-blocked',not revision(blocked) and len(native(f'/projects/{p}/commits'))==oldcount+1,blocked)
        except Unknown:
            if mode!='CRASH': raise
        if mode=='CRASH':
            state=json.loads(run(['podman','inspect',POD+'-owner']).stdout)[0]['State']
            gate('owner_process_crashed_after_acceptance',not state['Running'] and state['ExitCode']==86,state)
        accepted=native(f'/projects/{p}/branches/{b}')['head']['@id']
        gate('one_remote_commit-before-reconcile-'+mode,len(native(f'/projects/{p}/commits'))==oldcount+1)
        run(['bash',str(ROOT/'scripts/sysml-owner-runtime.sh'),'owner-restart'],timeout=100)
        code,reconciled=http('POST',f'/v1/projects/{project}/operations/{op}/reconcile',tokens['owner'])
        gate('restart_reconciliation-'+mode,code==200 and revision(reconciled)==accepted and
             len(native(f'/projects/{p}/commits'))==oldcount+1,reconciled)
        # Repeated reconciliation returns proof and cannot dispatch again.
        code,again=http('POST',f'/v1/projects/{project}/operations/{op}/reconcile',tokens['owner'])
        gate('reconciliation_no_duplicate-'+mode,code==200 and revision(again)==accepted and
             len(native(f'/projects/{p}/commits'))==oldcount+1,again)
    # Hydrate the exact latest accepted bundle into a separately provisioned project.
    current_head=native(f'/projects/{p}/branches/{b}')['head']['@id']
    _,source_bundle=http('GET',f'/v1/projects/{project}/revisions/{current_head}/bundle',tokens['reader'])
    fresh=next(x for x in config['projects'] if x['project']=='fresh-project')
    fp,fb=fresh['remote_project'],fresh['remote_branch']
    fh=native(f'/projects/{fp}/branches/{fb}').get('head')
    fh=fh.get('@id') if isinstance(fh,dict) else fh
    fresh_bundle=bundle('fresh-project','base',fh,existing=source_bundle)
    freshop=run_id+'-fresh'
    path=f'/v1/projects/fresh-project/branches/main/operations/{freshop}'
    code,value=http('POST',path,tokens['editor-a'],{'expected_head':{'kind':'revision','revision':fh} if fh else {'kind':'empty'},'bundle':fresh_bundle})
    gate('fresh-intake',code in (200,201,202),value)
    code,value=http('POST',path+'/promote',tokens['editor-a'])
    fr=revision(value)
    gate('fresh-actual-commit',code==200 and bool(fr),value)
    code,hydrated=http('GET',f'/v1/projects/fresh-project/revisions/{fr}/bundle',tokens['reader'])
    gate('fresh_project_hydration',code==200 and hydrated['model']==source_bundle['model'] and
         hydrated['manifest']['semantic_digest']==source_bundle['manifest']['semantic_digest'] and
         hydrated['blobs']==source_bundle['blobs'],{'revision':fr,'bundle':hydrated,
            'native':native(f'/projects/{fp}/commits/{fr}/elements?excludeUsed=false')})
    # Delete every managed element while retaining the operation-bound envelope anchor.
    empty_bundle=bundle('fresh-project','empty',fr,existing=hydrated)
    emptyop=run_id+'-empty'
    path=f'/v1/projects/fresh-project/branches/main/operations/{emptyop}'
    code,value=http('POST',path,tokens['editor-a'],{'expected_head':{'kind':'revision','revision':fr},'bundle':empty_bundle})
    gate('empty-intake',code in (200,201,202),value)
    code,value=http('POST',path+'/promote',tokens['editor-a'])
    er=revision(value)
    gate('deleted-all-actual-commit',code==200 and bool(er),value)
    code,empty_export=http('GET',f'/v1/projects/fresh-project/revisions/{er}/bundle',tokens['reader'])
    empty_native=native(f'/projects/{fp}/commits/{er}/elements?excludeUsed=false')
    gate('deleted_all_anchor_retained',code==200 and empty_export['model']=={'elements':{},'relations':{}} and
         empty_export['blobs']==source_bundle['blobs'] and len(empty_native)==1 and empty_native[0]['@type']=='Package',empty_native)
    REPORT['gates']['crash_reconciliation']={'status':'Satisfied','details':'exit86 + ambiguous response, reopened same DB, exact reachable history, no duplicate commits'}
    REPORT['status']='Satisfied'

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--output', default='/tmp/sysml-owner-c9d-live-report.json')
    args = parser.parse_args()
    exit_code = 2
    try:
        main()
        REPORT['status'] = 'Satisfied'; exit_code = 0
    except AssertionError as e:
        REPORT['status'] = 'Violated'; REPORT['error'] = str(e); exit_code = 1
    except (Unknown, OSError, ValueError, KeyError, subprocess.TimeoutExpired) as e:
        REPORT['error'] = str(e)
    finally:
        output = Path(args.output)
        output.parent.mkdir(parents=True,exist_ok=True)
        output.write_text(json.dumps(REPORT, ensure_ascii=False, indent=2, default=lambda x: {'bytes_sha256':hashlib.sha256(x).hexdigest()})+'\n')
        print(json.dumps({'status':REPORT['status'],'report':str(output),'error':REPORT.get('error')}))
    sys.exit(exit_code)
