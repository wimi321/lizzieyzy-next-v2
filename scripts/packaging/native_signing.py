#!/usr/bin/env python3
"""Optional publisher signing for release builds; credentials never enter artifacts."""
import argparse
import base64
import binascii
import contextlib
import json
import os
from pathlib import Path
import re
import secrets
import shlex
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
DEFAULT_DIRECTORY = ROOT / "target/native-signing"
MAC_CERTIFICATE = ("APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD", "APPLE_SIGNING_IDENTITY")
NOTARY_CREDENTIALS = ("APPLE_ID", "APPLE_PASSWORD", "APPLE_TEAM_ID")
WINDOWS_CERTIFICATE = ("WINDOWS_CERTIFICATE", "WINDOWS_CERTIFICATE_PASSWORD")
TIMESTAMP_URL = "http://timestamp.digicert.com"


class SigningError(RuntimeError):
    pass


def run(command, *, env=None):
    # Some commands contain import/notarization passwords. Never print argv, stdout,
    # stderr or CalledProcessError (which includes argv) on failure.
    result = subprocess.run(command, env=env, capture_output=True, text=True)
    if result.returncode:
        raise SigningError(f"{Path(command[0]).name} failed (exit {result.returncode}); "
                           "publisher signing/notarization was not completed")
    return result.stdout


def complete(env, names):
    return all(env.get(name, "").strip() for name in names)


def save_state(directory, state):
    (directory / "state.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")


def load_state(directory):
    path = directory / "state.json"
    if path.is_symlink() or not path.is_file():
        raise SigningError("Missing managed publisher-signing state; run setup first")
    state = json.loads(path.read_text(encoding="utf-8"))
    if state.get("schema") != "lizzie-native-signing-v1":
        raise SigningError("Unrecognized publisher-signing state")
    return state


def certificate_file(directory, encoded):
    try:
        data = base64.b64decode("".join(encoded.split()), validate=True)
    except (ValueError, binascii.Error) as error:
        raise SigningError("Certificate must be base64-encoded PKCS#12 data") from error
    if not data:
        raise SigningError("Certificate must not be empty")
    path = directory / "certificate.p12"
    with path.open("xb") as stream:
        stream.write(data)
    path.chmod(0o600)
    return path


def setup_macos(directory, state, env):
    if not complete(env, MAC_CERTIFICATE):
        print("macOS 发布者签名已跳过：缺少 Developer ID 证书、密码或身份；保留预览 ad-hoc 签名。")
        print("macOS 公证已跳过：未配置发布者签名。")
        return {"bundle": {"macOS": {"signingIdentity": "-"}}}
    identity = env["APPLE_SIGNING_IDENTITY"].strip()
    if not identity.startswith("Developer ID Application:") or "\n" in identity or "\r" in identity:
        raise SigningError("APPLE_SIGNING_IDENTITY must be a Developer ID Application identity")
    keychain = directory / "release.keychain-db"
    state["previousKeychains"] = shlex.split(run(["security", "list-keychains", "-d", "user"]))
    state["keychain"] = str(keychain)
    save_state(directory, state)  # Always-step cleanup also covers a partial import.
    certificate = certificate_file(directory, env["APPLE_CERTIFICATE"])
    password = secrets.token_urlsafe(32)
    try:
        run(["security", "create-keychain", "-p", password, str(keychain)])
        run(["security", "set-keychain-settings", "-lut", "21600", str(keychain)])
        run(["security", "unlock-keychain", "-p", password, str(keychain)])
        run(["security", "import", str(certificate), "-f", "pkcs12", "-k", str(keychain), "-P",
             env["APPLE_CERTIFICATE_PASSWORD"], "-T", "/usr/bin/codesign", "-T", "/usr/bin/security"])
        run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:,codesign:",
             "-s", "-k", password, str(keychain)])
        run(["security", "list-keychains", "-d", "user", "-s",
             *state["previousKeychains"], str(keychain)])
        identities = run(["security", "find-identity", "-v", "-p", "codesigning", str(keychain)])
        if f'"{identity}"' not in identities:
            raise SigningError("Imported certificate does not match the requested signing identity")
    finally:
        certificate.unlink(missing_ok=True)
    state["identity"] = identity
    print("macOS 发布者签名已启用：Developer ID、硬化运行时和安全时间戳。")
    return {"bundle": {"macOS": {"signingIdentity": identity, "hardenedRuntime": True}}}


def setup_windows(directory, state, env):
    if not complete(env, WINDOWS_CERTIFICATE):
        print("Windows 代码签名已跳过：缺少 WINDOWS_CERTIFICATE 或 WINDOWS_CERTIFICATE_PASSWORD。")
        return {}
    certificate = certificate_file(directory, env["WINDOWS_CERTIFICATE"])
    process_env = dict(env, LIZZIE_CERTIFICATE_PATH=str(certificate),
                       LIZZIE_CERTIFICATE_STATE=str(directory / "windows-thumbprint.txt"))
    # Save the thumbprint before import so cleanup can remove even a partial import.
    script = """
$ErrorActionPreference = 'Stop'
$password = ConvertTo-SecureString $env:WINDOWS_CERTIFICATE_PASSWORD -AsPlainText -Force
$pfx = Get-PfxData -FilePath $env:LIZZIE_CERTIFICATE_PATH -Password $password
$certificates = @($pfx.EndEntityCertificates)
if ($certificates.Count -ne 1) { throw 'Expected one code-signing certificate' }
$thumbprint = $certificates[0].Thumbprint
if (Test-Path ('Cert:\\CurrentUser\\My\\' + $thumbprint)) { throw 'Certificate already exists in user store' }
[IO.File]::WriteAllText($env:LIZZIE_CERTIFICATE_STATE, $thumbprint)
$certificate = Import-PfxCertificate -FilePath $env:LIZZIE_CERTIFICATE_PATH -Password $password -CertStoreLocation Cert:\\CurrentUser\\My
if (-not $certificate.HasPrivateKey) { throw 'Code-signing certificate has no private key' }
"""
    try:
        run(["pwsh", "-NoProfile", "-NonInteractive", "-Command", script], env=process_env)
        thumbprint = (directory / "windows-thumbprint.txt").read_text(encoding="utf-8").strip()
        if not re.fullmatch(r"[0-9A-Fa-f]{40}", thumbprint):
            raise SigningError("Imported certificate returned an invalid thumbprint")
    finally:
        certificate.unlink(missing_ok=True)
    state["thumbprint"] = thumbprint
    print("Windows 代码签名已启用：应用、安装器与捆绑 PE 文件均使用 SHA-256 和时间戳。")
    return {"bundle": {"windows": {"certificateThumbprint": thumbprint,
            "digestAlgorithm": "sha256", "timestampUrl": TIMESTAMP_URL, "tsp": True}}}


def setup(platform, directory, config, env=None):
    env = os.environ if env is None else env
    directory = directory.absolute()
    if directory.exists() or directory.is_symlink():
        raise SigningError("Refusing to overwrite existing publisher-signing directory; clean it first")
    directory.mkdir(parents=True, mode=0o700)
    state = {"schema": "lizzie-native-signing-v1", "platform": platform}
    save_state(directory, state)
    if platform.startswith("macos-"):
        override = setup_macos(directory, state, env)
    elif platform == "windows-x64":
        override = setup_windows(directory, state, env)
    elif platform == "linux-x64":
        override = {}
        print("Linux 无系统代码签名步骤；更新签名单独处理。")
    else:
        raise SigningError("Unsupported publisher-signing platform")
    save_state(directory, state)
    config.parent.mkdir(parents=True, exist_ok=True)
    config.write_text(json.dumps(override, indent=2) + "\n", encoding="utf-8")
    return state


def build_environment(state, env):
    result = dict(env)
    # Tauri must not re-import the certificate, accidentally discover another
    # signing identity, or treat a partial notarization group as configured.
    for name in (*MAC_CERTIFICATE, *NOTARY_CREDENTIALS, *WINDOWS_CERTIFICATE,
                 "APPLE_API_KEY", "APPLE_API_ISSUER", "APPLE_API_KEY_PATH"):
        result.pop(name, None)
    if state.get("identity") and complete(env, NOTARY_CREDENTIALS):
        result.update({name: env[name] for name in NOTARY_CREDENTIALS})
        print("macOS app 将由 Tauri 使用 notarytool 公证并 staple，然后生成 DMG。", flush=True)
    elif state["platform"].startswith("macos-"):
        print("macOS 公证已跳过：发布者签名或公证 secrets 不完整。", flush=True)
    return result


def build(directory, command):
    state = load_state(directory)
    env = build_environment(state, os.environ)
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        raise SigningError("A build command is required after --")
    # .cmd shims need resolution on Windows, without invoking an interpolated shell.
    command[0] = shutil.which(command[0]) or command[0]
    result = subprocess.run(command, env=env)
    if result.returncode:
        raise SigningError(f"Tauri release build failed (exit {result.returncode})")


def signtool():
    found = shutil.which("signtool.exe")
    if found:
        return found
    kits = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)")) / "Windows Kits/10/bin"
    candidates = sorted(kits.glob("*/x64/signtool.exe"))
    if not candidates:
        raise SigningError("Windows SDK signtool.exe was not found")
    return str(candidates[-1])


def sign_windows_resources(directory, state):
    if not state.get("thumbprint"):
        return
    tool = signtool()
    for path in sorted(directory.rglob("*")):
        if path.is_file() and path.suffix.lower() in (".exe", ".dll"):
            if path.is_symlink():
                raise SigningError("Refusing to sign a linked resource")
            run([tool, "sign", "/sha1", state["thumbprint"], "/s", "My", "/fd", "SHA256",
                 "/tr", TIMESTAMP_URL, "/td", "SHA256", str(path)])
            run([tool, "verify", "/pa", "/all", str(path)])


@contextlib.contextmanager
def mounted(dmg, directory):
    # A private, read-only mount: nothing opens in Finder and nothing is written.
    mount = Path(tempfile.mkdtemp(prefix="dmg-", dir=directory.absolute()))
    run(["hdiutil", "attach", "-nobrowse", "-readonly", "-noautoopen",
         "-mountpoint", str(mount), str(dmg)])
    try:
        yield mount
    finally:
        try:
            run(["hdiutil", "detach", str(mount)])
        except SigningError:
            run(["hdiutil", "detach", "-force", str(mount)])
        mount.rmdir()


def notarize(directory, dmg, env=None):
    env = os.environ if env is None else env
    state = load_state(directory)
    if not state.get("identity") or not complete(env, NOTARY_CREDENTIALS):
        print("macOS DMG 公证与 staple 已跳过：发布者签名或公证 secrets 不完整。")
        return False
    # Tauri notarizes and staples the app, puts it in the DMG, then removes the
    # standalone .app when only the DMG was requested. Check the copy users get.
    with mounted(dmg, directory) as volume:
        apps = [path for path in volume.iterdir()
                if path.suffix == ".app" and path.is_dir() and not path.is_symlink()]
        if len(apps) != 1:
            raise SigningError(f"Expected exactly one app in the DMG, found {len(apps)}")
        run(["codesign", "--verify", "--deep", "--strict", str(apps[0])])
        run(["xcrun", "stapler", "validate", str(apps[0])])
    run(["codesign", "--verify", "--strict", str(dmg)])
    response = run(["xcrun", "notarytool", "submit", str(dmg), "--apple-id", env["APPLE_ID"],
                    "--password", env["APPLE_PASSWORD"], "--team-id", env["APPLE_TEAM_ID"],
                    "--wait", "--timeout", "30m", "--output-format", "json"])
    if json.loads(response).get("status") != "Accepted":
        raise SigningError("Apple notarization did not accept the DMG; no release assets should be uploaded")
    run(["xcrun", "stapler", "staple", str(dmg)])
    run(["xcrun", "stapler", "validate", str(dmg)])
    print("macOS app 与 DMG 公证票据验证通过。")
    return True


def cleanup(directory):
    if not directory.exists():
        return
    if directory.is_symlink():
        raise SigningError("Refusing to clean a linked signing directory")
    state = load_state(directory)
    if state.get("keychain"):
        keychain = Path(state["keychain"])
        if keychain != directory.absolute() / "release.keychain-db":
            raise SigningError("Signing keychain path is outside its managed directory")
        run(["security", "list-keychains", "-d", "user", "-s", *state["previousKeychains"]])
        if keychain.exists():
            run(["security", "delete-keychain", str(keychain)])
    thumbprint_file = directory / "windows-thumbprint.txt"
    if thumbprint_file.exists():
        thumbprint = thumbprint_file.read_text(encoding="utf-8").strip()
        if not re.fullmatch(r"[0-9A-Fa-f]{40}", thumbprint):
            raise SigningError("Refusing to remove an invalid certificate thumbprint")
        run(["pwsh", "-NoProfile", "-NonInteractive", "-Command",
             "$ErrorActionPreference = 'Stop'; $path = 'Cert:\\CurrentUser\\My\\' + "
             "$env:LIZZIE_CERTIFICATE_THUMBPRINT; if (Test-Path $path) { Remove-Item -LiteralPath $path -DeleteKey }"],
            env=dict(os.environ, LIZZIE_CERTIFICATE_THUMBPRINT=thumbprint))
    shutil.rmtree(directory)
    print("已清理本次发布的临时证书、keychain 与签名状态。")


def main():
    # GitHub's Windows Python can default to a legacy code page when redirected.
    # These helpers intentionally log Chinese missing-secret explanations.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("setup", "build", "notarize", "cleanup"):
        command = commands.add_parser(name)
        command.add_argument("--directory", type=Path, default=DEFAULT_DIRECTORY)
        if name == "setup":
            command.add_argument("--platform", required=True)
            command.add_argument("--config", type=Path, required=True)
        elif name == "build":
            command.add_argument("build_command", nargs=argparse.REMAINDER)
        elif name == "notarize":
            command.add_argument("--dmg", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "setup":
            setup(args.platform, args.directory, args.config)
        elif args.command == "build":
            build(args.directory, args.build_command)
        elif args.command == "notarize":
            notarize(args.directory, args.dmg)
        else:
            cleanup(args.directory)
    except (SigningError, OSError, ValueError) as error:
        parser.exit(1, f"native signing: FAILED: {error}\n")


if __name__ == "__main__":
    main()
