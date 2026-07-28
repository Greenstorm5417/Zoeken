# CI helper: wrap a locally prepared release context directory
# (zoeken-server + assets/ + settings.yml + …) without fetching GitHub.
{
  lib,
  stdenvNoCC,
  autoPatchelfHook,
  makeWrapper,
  src,
  version ? "0.0.0",
}:

stdenvNoCC.mkDerivation {
  pname = "zoeken";
  inherit version src;

  dontUnpack = true;
  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];
  buildInputs = [ stdenvNoCC.cc.cc.lib ];

  installPhase = ''
    runHook preInstall

    install -Dm755 "$src/zoeken-server" "$out/libexec/zoeken-server"
    mkdir -p "$out/share/zoeken/assets" "$out/etc/zoeken"
    cp -R "$src/assets/." "$out/share/zoeken/assets/"
    install -Dm644 "$src/settings.yml" "$out/etc/zoeken/settings.yml"
    install -Dm644 "$src/limiter.toml" "$out/etc/zoeken/limiter.toml"
    install -Dm644 "$src/default.config.yml" "$out/share/doc/zoeken/default.config.yml"
    install -Dm644 "$src/LICENSE" "$out/share/licenses/zoeken/LICENSE"

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
    platforms = platforms.linux;
  };
}
