# Wraps a Zoeken GitHub release archive (binary + SPA + config).
# Used by the Zoeken flake and mirrored by nixos-pkgs.
{
  lib,
  stdenv,
  stdenvNoCC,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  generated ? import ./generated.nix,
}:

let
  system = stdenvNoCC.hostPlatform.system;
  source =
    generated.sources.${system}
      or (throw "zoeken: no prebuilt release for system '${system}' (have: ${lib.concatStringsSep ", " (lib.attrNames generated.sources)})");
in
stdenvNoCC.mkDerivation {
  pname = "zoeken";
  version = generated.version;

  src = fetchurl {
    inherit (source) url hash;
  };

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];
  buildInputs = [ stdenv.cc.cc.lib ];

  installPhase = ''
    runHook preInstall

    install -Dm755 zoeken-server "$out/libexec/zoeken-server"
    mkdir -p "$out/share/zoeken/assets" "$out/etc/zoeken"
    cp -R assets/. "$out/share/zoeken/assets/"
    install -Dm644 settings.yml "$out/etc/zoeken/settings.yml"
    install -Dm644 limiter.toml "$out/etc/zoeken/limiter.toml"
    install -Dm644 default.config.yml "$out/share/doc/zoeken/default.config.yml"
    install -Dm644 LICENSE "$out/share/licenses/zoeken/LICENSE"

    substituteInPlace "$out/etc/zoeken/settings.yml" \
      --replace-fail "/etc/zoeken/limiter.toml" "$out/etc/zoeken/limiter.toml"

    makeWrapper "$out/libexec/zoeken-server" "$out/bin/zoeken-server" \
      --set-default APP_ASSETS_DIR "$out/share/zoeken/assets" \
      --set-default APP_SETTINGS_PATH "$out/etc/zoeken/settings.yml"

    runHook postInstall
  '';

  meta = with lib; {
    description = "Privacy-respecting metasearch engine";
    homepage = "https://github.com/Greenstorm5417/Zoeken";
    license = licenses.agpl3Plus;
    mainProgram = "zoeken-server";
    platforms = lib.attrNames generated.sources;
  };
}
