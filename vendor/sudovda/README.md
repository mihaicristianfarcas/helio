# SudoVDA 1.10.9.289

The SudoMaker Virtual Display Adapter, the virtual display driver Helio Host
uses on Windows (an IddCx driver, user mode, x64 only). Helio Host installs
it when hosting is turned on and removes it when hosting is turned off, if
it was not there before (`helio-host/src/hosting/drivers.rs`, which embeds
these files).

| File | SHA-256 |
|---|---|
| `SudoVDA.inf` | `ad69ac682756f0cf339b081fac7e6e8159fdf2ca01ca69df8945c7246c286925` |
| `sudovda.cat` | `2f9189de5604bec9d86f51640cc540639e394d9ad0f8e689129375e95f2d22f8` |
| `SudoVDA.dll` | `47ee263cb5de9382c6630a2d7f3dafec4a49419f953beec869ca5dd0c460ff63` |
| `sudovda.cer` | `6accdcd519f6179d967db4eaa20ecf25a732ba30e87f4cffebc768b2c13c9007` |

## Where they come from

SudoVDA's source is [SudoMaker/SudoVDA](https://github.com/SudoMaker/SudoVDA),
which publishes no builds. These are the signed build that
[Apollo](https://github.com/ClassicOldSong/Apollo) ships: the files in
`drivers/sudovda/` of its `Apollo-0.4.6.exe` installer (release v0.4.6,
SHA-256 `42b2aefaacb3474511517a56b96ee9f0517f30ac38b5dd2fda9fd5b478f5021a`),
unchanged. To check them, extract that folder with 7-Zip
(`7z x Apollo-0.4.6.exe drivers/sudovda`) and compare the hashes above.

The driver and its catalog are signed by SudoMaker's own certificate,
`CN=sudovda@su.mk` (`sudovda.cer`, SHA-1 thumbprint
`3C918FC73525AD8B1521B6DB26B71F694277CC49`, valid until 2030-07-13), not by
Microsoft. Windows installs a user-mode driver signed so only from a
publisher it trusts, so installing it adds that certificate to the local
machine's Trusted Root and Trusted Publishers stores, as Apollo's
`install.bat` does. The certificate is for code signing only (its extended
key usage), so Windows trusts it for nothing else, such as a website.
Removing the driver removes the certificate again, when Helio Host added
it.

## License

SudoVDA's README: "MIT and CC0 or Public Domain (for changes I made, please
consult Microsoft for their license), choose the least restrictive option."
It is based on Microsoft's Indirect Display Driver sample
([microsoft/Windows-driver-samples](https://github.com/microsoft/Windows-driver-samples/tree/main/video/IndirectDisplay),
MS-PL). Helio ships it as a separate program beside its own, not as part
of Helio Host.
