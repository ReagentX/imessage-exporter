class ImessageExporter < Formula
  # change these var when a new version is released
  soft_version = "1.0.0"
  sha256sum_intel = "4d7f790d1da616e08cbd71a390362b1cf311b17a77386ee0ea9fe147944ab914"
  sha256sum_arm64 = "c0a20681d6c90a5d22314d585364de8d62def9579c8014acbe4bcaa13d6d23b6"

  # formula infos
  desc "CLI to export MacOS iMessage data + run diagnostics"
  repo_url = "https://github.com/ReagentX/imessage-exporter"
  homepage repo_url
  version soft_version

  binary_name = "imessage-exporter-" + (Hardware::CPU.type == :intel ? "x86_64" : "aarch64") + "-apple-darwin.tar.gz"
  binary_url = "#{repo_url}/releases/latest/download/#{binary_name}"
  url binary_url
  sha256 Hardware::CPU.type == :intel ? sha256sum_intel : sha256sum_arm64

  def install
    bin.install "imessage-exporter"
  end

  test do
    assert_equal "imessage is not a valid export type! Must be one of <txt, html>\n",
    shell_output("imessage-exporter -f imessage")
    assert_equal "Diagnostics are enabled; format is disallowed\n",
    shell_output("imessage-exporter -f txt -d")
    assert_equal "No export type selected, required by no-copy\n",
    shell_output("imessage-exporter -n")
    assert_equal "No export type selected, required by export-path\n",
    shell_output("imessage-exporter -o imessage")
    assert_equal "",
    shell_output("imessage-exporter -p imessage")
  end
end
