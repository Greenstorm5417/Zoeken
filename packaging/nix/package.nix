{ src, version ? "0.0.0" }:

let
  # NixOS 26.05, pinned so release packages do not drift with a channel.
  nixpkgs = builtins.fetchTarball
    "https://github.com/NixOS/nixpkgs/archive/fd1462031fdee08f65fd0b4c6b64e22239a77870.tar.gz";
  pkgs = import nixpkgs { };
in
pkgs.stdenvNoCC.mkDerivation {
  pname = "zoeken";
  inherit version src;

  dontUnpack = true;
  nativeBuildInputs = with pkgs; [ autoPatchelfHook makeWrapper ];
  buildInputs = [ pkgs.stdenv.cc.cc.lib ];

  installPhase = ''
    runHook preInstall

    install -Dm755 "$src/zoeken-server" "$out/libexec/zoeken-server"
    mkdir -p "$out/share/zoeken/assets" "$out/share/zoeken/plugins" "$out/etc/zoeken"
    cp -R "$src/assets/." "$out/share/zoeken/assets/"
    cp -R "$src/plugins/." "$out/share/zoeken/plugins/"
    install -Dm644 "$src/settings.yml" "$out/etc/zoeken/settings.yml"
    install -Dm644 "$src/limiter.toml" "$out/etc/zoeken/limiter.toml"
    install -Dm644 "$src/default.config.yml" "$out/share/doc/zoeken/default.config.yml"
    install -Dm644 "$src/LICENSE" "$out/share/licenses/zoeken/LICENSE"

    substituteInPlace "$out/etc/zoeken/settings.yml" \
      --replace-fail "/usr/share/zoeken/plugins" "$out/share/zoeken/plugins" \
      --replace-fail "/etc/zoeken/limiter.toml" "$out/etc/zoeken/limiter.toml"

    makeWrapper "$out/libexec/zoeken-server" "$out/bin/zoeken-server" \
      --set-default APP_ASSETS_DIR "$out/share/zoeken/assets" \
      --set-default APP_SETTINGS_PATH "$out/etc/zoeken/settings.yml"

    runHook postInstall
  '';

  meta = with pkgs.lib; {
    description = "Privacy-respecting metasearch engine";
    homepage = "https://github.com/Greenstorm5417/Zoeken";
    license = licenses.agpl3Plus;
    mainProgram = "zoeken-server";
    platforms = platforms.linux;
  };
}
