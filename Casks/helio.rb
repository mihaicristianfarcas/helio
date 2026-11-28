# Written by tools/update-casks for each release: change that script, not
# this file.
cask "helio" do
  arch arm: "arm64", intel: "x86_64"

  version "0.10.0"
  sha256 arm:   "b399e2e606570c36186fd96fdf71f09e56bcabef39479c1e7a21c41ad77bfccc",
         intel: "fa506d74cb5d1dba238faa32d879ce090d3c9f0fb7c611ec33b2651d9b381255"

  url "https://github.com/mihaicristianfarcas/helio/releases/download/v#{version}/Helio-#{version}-macos-#{arch}.zip"
  name "Helio"
  desc "Desktop and game streaming, client and host, post-quantum encrypted"
  homepage "https://github.com/mihaicristianfarcas/helio"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :sonoma

  app "Helio.app"

  uninstall quit: "dev.mihaicristian.helio"

  zap launchctl: "dev.mihaicristian.helio.host",
      delete:    "/etc/paths.d/helio",
      trash:     [
        "~/Library/Application Support/Helio Host",
        "~/Library/Application Support/Helio",
        "~/Library/LaunchAgents/dev.mihaicristian.helio.host.plist",
        "~/Library/LaunchAgents/dev.mihaicristian.helio.plist",
        "~/Library/Logs/Helio",
        "~/Library/Preferences/dev.mihaicristian.helio.plist",
      ]

  caveats <<~EOS
    Hosting is off until you turn it on in Helio (Settings > Hosting). macOS
    then asks for Screen Recording and Accessibility for Helio Host
    inside Helio.
  EOS
end
