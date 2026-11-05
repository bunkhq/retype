#!/usr/bin/env python3
"""Copy ffmpeg and ffprobe, plus every non-system dylib they load, into a
Retype.app so it runs on a Mac without Homebrew.

    scripts/embed_ffmpeg.py dist/Retype.app

Binaries go to Contents/Resources (where Retype looks first), libraries to
Contents/Frameworks, and every load path is rewritten to point inside the app.

Licence: a Homebrew ffmpeg is built with GPL components (x264, x265). An app
that ships it must be distributed under GPL terms. For a closed-source release,
build ffmpeg with --disable-gpl --disable-nonfree and point FFMPEG_BIN_DIR at it.
"""

import os
import shutil
import subprocess
import sys

SYSTEM = ("/usr/lib/", "/System/")


def deps(path):
    out = subprocess.run(["otool", "-L", path], capture_output=True, text=True, check=True).stdout
    lines = out.splitlines()[1:]
    return [l.strip().split(" (")[0] for l in lines if l.strip()]


def rpaths(path):
    out = subprocess.run(["otool", "-l", path], capture_output=True, text=True, check=True).stdout.splitlines()
    found = []
    for i, l in enumerate(out):
        if "cmd LC_RPATH" in l:
            for j in range(i, min(i + 4, len(out))):
                if out[j].strip().startswith("path "):
                    found.append(out[j].strip().split(" ")[1])
    return found


def resolve(ref, origin):
    """Turn a load path into a real file, given the file that loads it."""
    here = os.path.dirname(origin)
    if ref.startswith("@loader_path/") or ref.startswith("@executable_path/"):
        cand = os.path.join(here, ref.split("/", 1)[1])
        return os.path.realpath(cand) if os.path.exists(cand) else None
    if ref.startswith("@rpath/"):
        name = ref.split("/", 1)[1]
        dirs = [p.replace("@loader_path", here).replace("@executable_path", here) for p in rpaths(origin)]
        dirs += ["/opt/homebrew/lib", "/usr/local/lib"]
        for d in dirs:
            cand = os.path.join(d, name)
            if os.path.exists(cand):
                return os.path.realpath(cand)
        return None
    return os.path.realpath(ref) if os.path.exists(ref) else None


def run(*cmd):
    subprocess.run(cmd, check=True, capture_output=True)


def main():
    app = sys.argv[1]
    res = os.path.join(app, "Contents", "Resources")
    fw = os.path.join(app, "Contents", "Frameworks")
    os.makedirs(fw, exist_ok=True)
    bin_dir = os.environ.get("FFMPEG_BIN_DIR")
    queue = []  # (copied file, original file, is_binary)
    for tool in ("ffmpeg", "ffprobe"):
        src = os.path.join(bin_dir, tool) if bin_dir else shutil.which(tool)
        if not src:
            sys.exit(f"{tool} not found")
        src = os.path.realpath(src)
        dst = os.path.join(res, tool)
        shutil.copy2(src, dst)
        os.chmod(dst, 0o755)
        queue.append((dst, src, True))

    seen = {}
    while queue:
        copied, original, is_bin = queue.pop()
        for ref in deps(original):
            if ref.startswith(SYSTEM):
                continue
            real = resolve(ref, original)
            if real is None:
                sys.exit(f"can't find {ref} (needed by {original})")
            name = os.path.basename(real)
            if os.path.realpath(original) == real:
                continue  # a dylib's own id line
            if name not in seen:
                dst = os.path.join(fw, name)
                shutil.copy2(real, dst)
                os.chmod(dst, 0o755)
                run("install_name_tool", "-id", f"@rpath/{name}", dst)
                seen[name] = dst
                queue.append((dst, real, False))
            new = f"@executable_path/../Frameworks/{name}" if is_bin else f"@loader_path/{name}"
            run("install_name_tool", "-change", ref, new, copied)

    # Strip build-machine rpaths, then re-sign (install_name_tool invalidates signatures).
    for path in [os.path.join(res, t) for t in ("ffmpeg", "ffprobe")] + list(seen.values()):
        for rp in rpaths(path):
            if rp.startswith("/opt/") or rp.startswith("/usr/local"):
                subprocess.run(["install_name_tool", "-delete_rpath", rp, path], capture_output=True)
        run("codesign", "--force", "--sign", os.environ.get("RETYPE_SIGN_ID") or "-", path)

    total = sum(os.path.getsize(p) for p in [os.path.join(res, "ffmpeg"), os.path.join(res, "ffprobe")] + list(seen.values()))
    print(f"Embedded ffmpeg, ffprobe and {len(seen)} libraries ({total / 1e6:.0f} MB)")


if __name__ == "__main__":
    main()
