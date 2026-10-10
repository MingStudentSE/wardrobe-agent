#!/usr/bin/env python3
"""#35 privacy spot check: scan release artifacts for credentials, EXIF/GPS
metadata and original photos. Writes a JSON report; exit non-zero on finding."""
import base64, json, os, pathlib, re, struct, sys

targets = {
    "dmg-app": pathlib.Path(sys.argv[1]),       # mounted .app
    "portable-cli": pathlib.Path(sys.argv[2]),  # portable dir
}
res = {}
cred_patterns = [
    ("sk-style api key", re.compile(rb"sk-[A-Za-z0-9]{16,}")),
    ("aws access key", re.compile(rb"AKIA[0-9A-Z]{16}")),
    ("private key block", re.compile(rb"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
    ("bearer token literal", re.compile(rb"Bearer [A-Za-z0-9_\-\.]{20,}")),
    ("64hex-ish constant", re.compile(rb"\b[0-9a-f]{64}\b")),
    ("password literal key", re.compile(rb"(password|passwd|secret)\s*[:=]\s*['\"][^'\"]{6,}", re.I)),
]
def image_files(root):
    for p in root.rglob("*"):
        if p.is_file() and p.suffix.lower() in (".png", ".jpg", ".jpeg", ".webp", ".heic", ".tif", ".tiff", ".icns"):
            yield p
def exif_gps_check(path):
    data = path.read_bytes()
    info = {"path": str(path), "bytes": len(data)}
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        info["format"] = "png"
        # PNG text chunks that might carry EXIF
        info["has_eXIf_chunk"] = b"eXIf" in data
        info["has_exif_marker"] = b"Exif" in data
    elif data[:3] == b"\xff\xd8\xff":
        info["format"] = "jpeg"
        info["has_exif_marker"] = b"Exif\x00\x00" in data
        # GPS IFD tag 0x8825 little/big endian occurrences
        info["gps_tag_present"] = b"\x25\x88" in data or b"\x88\x25" in data
    elif data[:4] == b"icns":
        info["format"] = "icns"
        info["has_exif_marker"] = False
    else:
        info["format"] = "other:" + data[:4].hex()
        info["has_exif_marker"] = b"Exif\x00\x00" in data
    return info
report = {"targets": {}}
overall_ok = True
for name, root in targets.items():
    files = sorted(p for p in root.rglob("*") if p.is_file())
    cred_hits = []
    image_report = []
    large_images = []
    onnx = []
    binaries = 0
    for p in files:
        data = None
        try:
            data = p.read_bytes()
        except Exception as e:
            cred_hits.append({"path": str(p), "error": str(e)})
            continue
        if p.suffix in (".dylib", "", ".o") or b"\xcf\xfa\xed\xfe" in data[:4096] or b"\x00" in data[:8192]:
            binaries += 1
        for label, pattern in cred_patterns:
            for m in pattern.finditer(data):
                cred_hits.append({"path": str(p.relative_to(root)), "pattern": label,
                                  "context": m.group(0)[:40].decode("utf-8", "replace")})
        if p.suffix.lower() == ".onnx" or "u2net" in p.name.lower():
            onnx.append(str(p.relative_to(root)))
    for p in image_files(root):
        info = exif_gps_check(p)
        rel = str(p.relative_to(root))
        info["path"] = rel
        image_report.append(info)
        if p.stat().st_size > 1_500_000:
            large_images.append(rel)
    # Public build metadata, not credentials (verified by context inspection):
    # - SQLite amalgamation SQLITE_SOURCE_ID hash (sits among sqlite string constants)
    # - U2NetP model SHA256 printed in the shipped public NOTICE
    allow_public = ("96c92aba00c8375bc32fafcdf12429c58bd8aabfcadab6683e35bbb9cdebf19e",
                    "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8")
    def is_public(h):
        c = h.get("context", "").strip()
        return bool(c) and any(c in a for a in allow_public)
    real_cred = [h for h in cred_hits if not is_public(h)]
    analyzed = [{**h, "classification": "public_build_metadata" if is_public(h) else "unresolved"}
                for h in cred_hits]
    report["targets"][name] = {
        "files_total": len(files),
        "binary_files": binaries,
        "credential_hits": cred_hits,
        "credential_hits_analyzed": analyzed,
        "credential_unresolved": real_cred,
        "images": image_report,
        "images_over_1_5mb": large_images,
        "model_weights_present": onnx,
    }
    if real_cred or large_images or onnx or any(i.get("has_exif_marker") or i.get("gps_tag_present") or i.get("has_eXIf_chunk") for i in image_report):
        overall_ok = False
report["overall_clean"] = overall_ok
out = pathlib.Path(sys.argv[3])
out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
print(json.dumps({"overall_clean": overall_ok,
                  "summary": {k: {"files": v["files_total"], "cred_hits": len(v["credential_hits"]),
                                   "images": len(v["images"]), "large": len(v["images_over_1_5mb"]),
                                   "onnx": v["model_weights_present"]}
                               for k, v in report["targets"].items()}}, ensure_ascii=False))
