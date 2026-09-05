#!/usr/bin/env bash
set -euo pipefail
workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
renderer="$workspace/crates/appstruct-codegen/templates/report-renderer"
image="appstruct-renderer-acceptance:${APPSTRUCT_RENDERER_TEST_TAG:-local}"
command -v docker >/dev/null
docker build --tag "$image" "$renderer"
isolation=(--rm --network none --read-only --user 10001:10001 --cap-drop ALL
  --security-opt no-new-privileges:true --security-opt "seccomp=$renderer/seccomp.json"
  --pids-limit 128 --memory 512m --memory-swap 512m --cpus 1 --shm-size 64m --tmpfs /tmp:size=128m,mode=1777)
docker run "${isolation[@]}" --entrypoint node "$image" --test /opt/renderer/test.mjs
docker run "${isolation[@]}" --entrypoint node "$image" --input-type=module -e '
  import assert from "node:assert/strict";
  import {readFile, writeFile} from "node:fs/promises";
  assert.notEqual(process.getuid(), 0);
  assert.equal((await readFile("/sys/fs/cgroup/pids.max", "utf8")).trim(), "128");
  assert.equal((await readFile("/sys/fs/cgroup/memory.max", "utf8")).trim(), "536870912");
  await assert.rejects(writeFile("/opt/renderer/forbidden", "x"));
  for (const url of ["http://169.254.169.254/latest/meta-data/", "http://1.1.1.1/", "https://example.com/", "http://127.0.0.1:3000/"]) {
    await assert.rejects(fetch(url, {signal: AbortSignal.timeout(1000)}));
  }
  console.log("Non-root, filesystem, network and cgroup isolation verified");
'
docker run "${isolation[@]}" --entrypoint node "$image" --input-type=module -e '
  import assert from "node:assert/strict";
  import {spawn} from "node:child_process";
  const children = [];
  let limited = false;
  try {
    for (let index = 0; index < 160; index++) {
      const child = spawn("/bin/sleep", ["10"], {stdio: "ignore"});
      const error = await new Promise(resolve => {child.once("spawn", () => resolve(null));child.once("error", resolve)});
      if (error) {assert.equal(error.code, "EAGAIN");limited = true;break;}
      children.push(child);
    }
    assert.equal(limited, true);
  } finally {for (const child of children) child.kill();}
'
status=0
docker run "${isolation[@]}" --entrypoint node "$image" --max-old-space-size=2048 -e \
  'const buffers=[]; for (;;) buffers.push(Buffer.alloc(32*1024*1024, 1));' || status=$?
[[ "$status" == 137 ]]
echo "Renderer isolation acceptance passed"
