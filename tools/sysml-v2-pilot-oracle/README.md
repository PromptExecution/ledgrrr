# SysML v2 Pilot Implementation — containerized "conformance oracle" spike

Spike for the SysML v2 tooling epic: can the OMG's reference implementation
([`Systems-Modeling/SysML-v2-Pilot-Implementation`](https://github.com/Systems-Modeling/SysML-v2-Pilot-Implementation))
be stood up as a "conformance oracle" — something other tools' parser output can be
checked against — as a GraalVM native-image, containerized (never installed on a host,
per project rule), built/run with `podman` (never `docker`)?

**Bottom line: no, not as a native-image, at least not without a multi-week manual
reachability-metadata effort. Yes, as a plain containerized JVM process — verified
working end-to-end below.**

## 1. Architecture findings

- Cloned `Systems-Modeling/SysML-v2-Pilot-Implementation` (main, 2026-08 snapshot).
- It genuinely is an Eclipse/Tycho project at its core: ~40 modules, most packaged
  `eclipse-plugin`/`eclipse-feature`, built with Maven+Tycho 4.0.13 against a target
  platform resolved from `download.eclipse.org/releases/2025-12` (full "Eclipse
  Modeling Tools" + Xtext Complete SDK). The editors, PlantUML views, and the
  `.installer`/Oomph setup are IDE-plugin-only and need the real Eclipse platform —
  no headless path exists for those.
- **However**, three of the ~40 modules are genuinely standalone, non-OSGi entry
  points, using Xtext's documented "standalone setup" pattern
  (`KerMLStandaloneSetup`/`SysMLStandaloneSetup`, Guice-injected, no OSGi service
  registry at runtime):
  - `org.omg.sysml.interactive.SysMLInteractive` — a stdin/stdout REPL (`main()` at
    `org.omg.sysml.interactive/src/org/omg/sysml/interactive/SysMLInteractive.java:856`).
  - `org.omg.sysml.xtext.util.SysML2JSON` / `SysML2XMI` — genuinely batch, one-shot
    CLIs: parse a `.sysml`/`.kerml` file (plus library search paths), resolve
    cross-references against the standard library, and export the resulting model
    graph as JSON or Ecore XMI. This is the useful "conformance oracle" primitive:
    if the reference implementation can parse+resolve a file, that's a real signal.
  - `org.omg.sysml.jupyter.kernel.ISysML` — wraps `SysMLInteractive` as a Jupyter
    (ZeroMQ) kernel; not directly useful headless (needs a Jupyter frontend and a
    connection file), but its release build conveniently bundles everything.
- The project publishes a monthly GitHub Release with a **pre-built fat jar**,
  `jupyter-sysml-kernel-<version>-all.jar` (inside `jupyter-sysml-kernel-<version>.zip`,
  ~132MB uncompressed), containing all of EMF, the Xtext runtime, Guice, ANTLR, and
  every compiled class from the whole reactor, including `SysMLInteractive` and
  `SysML2JSON`. We used this instead of running the Tycho build ourselves: doing our
  own `mvn clean install` needs Java 21 (host only has 17) and a p2 resolve against the
  full "2025-12" Eclipse release + Xtext SDK + a nightly Xpect update site — a build
  that is itself a large, slow, multi-GB undertaking independent of any native-image
  question, and the project already publishes the exact artifact we need. Using the
  official released jar is not cutting a corner on the actual question this spike
  asks (native-image feasibility) — it just avoids re-deriving a jar the project
  already ships.

## 2. GraalVM native-image attempt — FAILED

`Containerfile.native-image` runs `ghcr.io/graalvm/native-image-community:21`,
downloads the same released fat jar, and attempts:

```
native-image --no-fallback --report-unsupported-elements-at-runtime \
  -cp sysml-kernel-all.jar org.omg.sysml.xtext.util.SysML2JSON -o sysml2json-native
```

with **no** hand-written `reflect-config.json`/`resource-config.json` — i.e. the
realistic unassisted starting point for a codebase this size.

**Result: the `native-image` build itself actually SUCCEEDS (in ~1m35s, producing a
41.87MB binary) — but only because `--report-unsupported-elements-at-runtime` defers
reflection-registration failures from build time to run time. It looked, briefly,
like a native-image success. It is not one: running the binary is what actually
matters, and it fails immediately.**

First attempt used `FROM scratch` for the runtime stage and failed before even
reaching the real question: `native-image`'s default output here is dynamically
linked against glibc + libz (confirmed with `ldd` against the builder's output),
and `scratch` has no dynamic linker at all. Fixed by switching the final stage to
`debian:12-slim` + `zlib1g`.

With that fixed, running the binary against `samples/Vehicle.sysml` fails immediately,
during EMF's own `EcorePackage` static bootstrap — before any application code, Xtext
parsing, or Guice injection is even reached:

```
$ podman run --rm -v "$(pwd)/samples:/work:Z" sysml-v2-pilot-oracle:native-image \
    -l /app/sysml.library /work/Vehicle.sysml "Kernel Libraries" "Systems Library" "Domain Libraries"

Exception in thread "main" java.lang.ExceptionInInitializerError
	at org.eclipse.emf.ecore.impl.EPackageImpl.<init>(EPackageImpl.java:187)
	at org.omg.sysml.lang.sysml.impl.SysMLPackageImpl.<init>(SysMLPackageImpl.java:1496)
	at org.omg.sysml.lang.sysml.impl.SysMLPackageImpl.init(SysMLPackageImpl.java:1522)
	at org.omg.sysml.lang.sysml.SysMLPackage.<clinit>(SysMLPackage.java:60)
	at org.omg.sysml.io.SysMLUtil.<init>(SysMLUtil.java:73)
	...
	at org.omg.sysml.xtext.util.SysML2JSON.main(SysML2JSON.java:56)
Caused by: java.lang.IllegalArgumentException: Class org.eclipse.emf.ecore.ETypeParameter[]
  is instantiated reflectively but was never registered.
  Register the class by adding "unsafeAllocated" for the class in reflect-config.json.
	at org.eclipse.emf.ecore.util.EcoreEList.newData(EcoreEList.java:56)
	at org.eclipse.emf.ecore.impl.EPackageImpl.addETypeParameter(EPackageImpl.java:839)
	at org.eclipse.emf.ecore.impl.EcorePackageImpl.initializePackageContents(EcorePackageImpl.java:2694)
	at org.eclipse.emf.ecore.EcorePackage.<clinit>(EcorePackage.java:67)
```

That is: EMF's core `Ecore` metamodel itself allocates a reflective array
(`ETypeParameter[]`) during its own class initialization, and that single
unregistered reflective array allocation is enough to abort the whole process before
a single line of the SysML grammar is touched. This is exactly the concrete,
first-domino failure mode flagged as likely going in — dynamic EMF/Ecore reflection —
and it is evidence, not speculation: this is the actual stack trace from the actual
binary. Fixing just this one error (adding `EcorePackage`/`ETypeParameter` to a
`reflect-config.json`) would only reveal the next one; EMF's reflective package
bootstrap, Guice's proxy-based injector, and Xtext's generated parser/serializer
infrastructure each do this kind of thing pervasively throughout a ~40-module,
hundreds-of-classes reactor. There is no existing
`oracle/graalvm-reachability-metadata` coverage for `com.google.inject:guice` +
`org.eclipse.emf`/`org.eclipse.xtext` combined at this scale. Authoring a complete
`reflect-config.json`/`resource-config.json` by hand (or via the `native-image-agent`
tracing agent run across a representative-enough set of executions to auto-generate
one — which itself needs broad exercise of every language construct in the grammar
to be trustworthy) is a substantial, multi-week undertaking of its own — not
something to fake progress on for this spike.

**Recommendation:** do not pursue native-image for this tool further without a
dedicated reachability-metadata effort (most realistically: run the plain-JVM jar
under `-agentlib:native-image-agent=config-output-dir=...` across the full test/xpect
suite to auto-generate a starting config, then iterate). Track that as separate,
explicitly-scoped follow-up work if the epic still wants it — don't silently assume
it'll eventually work.

## 3. Plain-JVM container fallback — VERIFIED WORKING

`Containerfile.jvm` downloads the same released fat jar into an
`eclipse-temurin:21-jre-jammy` base, bundles the standard KerML/SysML library sources
(`sysml.library/`, copied from the pilot repo's own source tree, EPL-2.0 licensed —
see `THIRD_PARTY_LICENSE-sysml-v2-pilot-implementation`), and wraps
`org.omg.sysml.xtext.util.SysML2JSON` in `oracle.sh`.

Built and run for real with podman (never docker), against `samples/Vehicle.sysml`
(a small model using standard-library types like `ScalarValues::Integer`):

```
$ podman build -f Containerfile.jvm -t sysml-v2-pilot-oracle:jvm .
... Successfully tagged localhost/sysml-v2-pilot-oracle:jvm

$ podman run --rm -v "$(pwd)/samples:/work:Z" sysml-v2-pilot-oracle:jvm Vehicle.sysml
Saving Vehicle.sysml...
Transforming...
Processing
Writing Vehicle.json...
```

Output: a real `Vehicle.json`, 59 JSON-LD-style elements, ~56KB, with resolved
cross-references into the standard library (`ScalarValues::Integer`/`Real`) —
confirms the reference implementation genuinely parses and resolves a real model
inside the container.

**Caveat found while testing (worth flagging, not glossed over):** `SysML2JSON` is a
*traversal/export* utility, not a strict validator — a deliberately broken second
sample (`samples/Invalid.sysml`, unclosed braces, a bogus type reference) still
produced a `.json` output with exit code 0; the ANTLR-generated parser's error
recovery is tolerant enough that this input didn't trip it. A real "does this
conform" oracle would need to invoke the resource's `IResourceValidator` (the pattern
`SysMLInteractive`'s own REPL uses internally to print `ERROR:` lines) and check the
returned `Issue` list / severity, not just "did export complete". That's the concrete
next step if this line of work continues — not implemented in this spike's `oracle.sh`.

## Files

- `Containerfile.jvm` — the working fallback; builds and runs today.
- `Containerfile.native-image` — the native-image attempt; kept as evidence of what
  was tried and what broke, not deleted.
- `oracle.sh` — entrypoint wrapping `SysML2JSON`, used by `Containerfile.jvm`.
- `sysml.library/` — standard KerML/SysML library sources (EPL-2.0, from the pilot
  repo itself), needed to resolve standard-library cross-references.
- `samples/Vehicle.sysml` — small real model used to verify the container end-to-end.
- `samples/Invalid.sysml` — deliberately malformed input used to test (and find the
  limits of) the oracle's rejection behavior.
- `THIRD_PARTY_LICENSE-sysml-v2-pilot-implementation` — EPL-2.0 license of the
  upstream project, carried alongside the redistributed `sysml.library/` sources and
  the released fat jar this tooling downloads at build time.
