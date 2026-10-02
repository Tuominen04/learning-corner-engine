# Fonts

`DejaVuSans.ttf` is DejaVu Sans 2.37, exactly as released, from
<https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37>
(`dejavu-fonts-ttf-2.37.zip`, `ttf/DejaVuSans.ttf`).

| File | SHA-256 |
| --- | --- |
| `DejaVuSans.ttf` | `7da195a74c55bef988d0d48f9508bd5d849425c1770dba5d7bfc6ce9ed848954` |
| `LICENSE` | `7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93` |

Every PDF the engine writes is set in it, and carries the part of it the document uses
(`src/export/pdf/font.rs`). It is bundled rather than looked for on the computer because a font
found on one computer may be missing on the next.

Its licence, in `LICENSE`, is Bitstream Vera's, with the DejaVu changes in the public domain.
It lets the font be embedded and passed on, changed or not, provided its notices go with every
copy and a changed copy is not named "Bitstream" or "Vera". The part of the font inside a PDF is
such a copy: it keeps the font's own copyright and licence notices, which a test in `font.rs`
checks, and its name is "DejaVu Sans".

Replace the file only with another release, unchanged, and update the table above.
