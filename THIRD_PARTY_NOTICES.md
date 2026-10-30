# Space Zoo — Third-Party Notices

Space Zoo, Linux/X11: the following Rust dependencies are used under
MIT. Rust standard-library notices are in `RUST_TOOLCHAIN_NOTICES.md` and
`COPYRIGHT-library.html`.

## Package notices

| Crate | Pinned version | Upstream copyright / license-file notice |
|---|---:|---|
| `bitflags` | 2.13.2 | Copyright (c) 2014 The Rust Project Developers |
| `cfg-if` | 1.0.5 | Copyright (c) 2014 Alex Crichton |
| `crossterm` | 0.27.0 | Copyright (c) 2019 Timon |
| `errno` | 0.3.14 | Copyright (c) 2014 Chris Wong |
| `libc` | 0.2.159 | Copyright (c) The Rust Project Developers |
| `lock_api` | 0.4.14 | Copyright (c) 2016 The Rust Project Developers |
| `log` | 0.4.34 | Copyright (c) 2014 The Rust Project Developers |
| `minifb` | 0.27.0 | Copyright (c) 2016 Daniel Collin |
| `mio` | 0.8.11 | Copyright (c) 2014 Carl Lerche and other MIO contributors |
| `once_cell` | 1.21.4 | Upstream LICENSE-MIT contains no explicit copyright line. |
| `parking_lot` | 0.12.5 | Copyright (c) 2016 The Rust Project Developers |
| `parking_lot_core` | 0.9.12 | Copyright (c) 2016 The Rust Project Developers |
| `raw-window-handle` | 0.6.2 | Copyright (c) 2019 Osspial |
| `scopeguard` | 1.2.0 | Copyright (c) 2016-2019 Ulrik Sverdrup "bluss" and scopeguard developers |
| `signal-hook` | 0.3.18 | Copyright (c) 2017 tokio-jsonrpc developers |
| `signal-hook-mio` | 0.2.5 | Copyright (c) 2017 tokio-jsonrpc developers |
| `signal-hook-registry` | 1.4.8 | Copyright (c) 2017 tokio-jsonrpc developers |
| `smallvec` | 1.16.3 | Copyright (c) 2018 The Servo Project Developers |
| `x11-dl` | 2.21.0 | Upstream LICENSE-MIT contains no explicit copyright line. |

## MIT permission and disclaimer

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Natural Earth / DataHub ADMIN-1

Source: https://datahub.io/core/geo-ne-admin1

Natural Earth map data is public domain. The DataHub package is marked
ODC-PDDL-1.0: https://opendatacommons.org/licenses/pddl/1-0/

Space Zoo normalizes administrative names, codes, bounding boxes, and polygon
geometry into its runtime table and adds project-created water and fictional
geometry. The upstream DataHub attribution follows:

Attribution for geo-ne-admin1

Contains information from Natural Earth (https://www.naturalearthdata.com/), public domain under its terms of use (https://www.naturalearthdata.com/about/terms-of-use/).

Contains name translations that Natural Earth took from Wikidata (https://www.wikidata.org/), licensed under CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/).
Contains identifiers and names that Natural Earth took from GeoNames, the US NGA GNS, WOE (Yahoo/Flickr "Where on Earth"), HASC and GADM (the "type_en" column). Licence: unclear (none stated by Natural Earth for these columns; see the README).

Changes made to the source data: the file is downloaded from the Natural Earth repository and republished with one change: the top-level "crs" member is removed. All features and properties are otherwise unchanged.
Not endorsed by, or affiliated with, Natural Earth, Wikidata, GeoNames, GADM or the other providers named above.

Natural Earth provides its data as a public service and is not responsible for problems relating to accuracy, content, design, or how it is used.
