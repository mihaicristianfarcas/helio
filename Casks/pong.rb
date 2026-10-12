# Written by tools/update-casks for each release: change that script, not
# this file.
cask "pong" do
  arch arm: "arm64", intel: "x86_64"

  version "0.9.5"
  sha256 arm:   "4ee68ea9ae4f8fc22b1fe605e1568f7678350986e8c1ec2fb79018d1c04fc989",
         intel: "8231b8fc16c2ee03675081d815b0d703bf622db52a1ed69275d5f5f21cd1fe17"

  url "https://github.com/mihaicristianfarcas/pingpong/releases/download/v#{version}/Pong-#{version}-macos-#{arch}.zip"
  name "Pong"
  desc "Desktop and game streaming host for Ping clients, post-quantum encrypted"
  homepage "https://github.com/mihaicristianfarcas/pingpong"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: :sonoma

  app "Pong.app"
  app "Pong Control.app"

  uninstall launchctl: [
              "dev.pingpong.Pong",
              "dev.pingpong.PongControl",
            ],
            quit:      "dev.pingpong.PongControl"

  zap trash: [
    "~/Library/Application Support/Pong",
    "~/Library/Logs/Pong",
  ]

  caveats <<~EOS
    Open Pong Control and choose Start Pong: the host then runs whenever you
    are logged in, and Pong's icon is in the menu bar. macOS asks for Screen
    Recording and Accessibility the first time.
  EOS
end
