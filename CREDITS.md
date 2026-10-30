# Space Zoo — Credits, Licenses, and Notices

**Space Zoo / Hoyt Dynamic Laboratories**  
Copyleft (C) 2026 Hoyt Dynamic Laboratories.

## 1. Project license and third-party terms

- HDL-authored copyrighted software and eligible original project material:
  **GPL-2.0-or-later**, to the extent HDL holds rights. The authoritative
  software license declaration and complete GPL version 2 text are in `LICENSE.md`.
- Third-party source material, datasets, code, or adaptations retain their own
  applicable conditions and are **not** relicensed by the project GPL.
- Astronomy catalogue content below is recorded under **CC BY-NC 3.0 IGO**;
  its noncommercial limitation applies independently of the program license.
- All names of source institutions and contributors are acknowledgements only,
  not claims of endorsement.

## 2. Astronomy catalogue attribution

### Tycho-2

- **Authors / original work:** E. Høg; C. Fabricius; V. V. Makarov; S. E. Urban; T. E. Corbin; G. L. Wycoff; U. Bastian; P. Schwekendiek; A. Wicenec (2000), *The Tycho-2 Catalogue of the 2.5 Million Brightest Stars*.
- **Source:** CDS/VizieR I/259: https://cdsarc.cds.unistra.fr/viz-bin/cat/I/259
- **License:** Creative Commons Attribution–NonCommercial 3.0 IGO,
  https://creativecommons.org/licenses/by-nc/3.0/igo/legalcode
- **Use / modifications:** records selected and normalized into Space Zoo's
  compact Tycho catalogue and derived HIP identity, lookup, culture-match,
  render-eligibility and visibility data.
- **Conditions:** attribution, reference to license, identification of changes,
  noncommercial use only, and no implied endorsement. The project GPL does not
  confer commercial redistribution permission for this covered catalogue data.

### Hipparcos

- **Author / original work:** European Space Agency (1997),
  *The Hipparcos and Tycho Catalogues*, ESA SP-1200.
- **Scientific contributors:** M. A. C. Perryman; E. Høg; J. Kovalevsky; F. van Leeuwen; L. Lindegren; F. Mignard; H. Schrijver; C. Turon; and the Hipparcos scientific consortia and working groups.
- **Source:** CDS/VizieR I/239:
  https://cdsarc.cds.unistra.fr/viz-bin/Cat?cat=I%2F239
- **License:** CC BY-NC 3.0 IGO,
  https://creativecommons.org/licenses/by-nc/3.0/igo/legalcode
- **Use / modifications:** selected cross-identifiers and astrometric reference
  values normalized and indexed into lookup and reference/fixture formats.
- **Conditions:** credit ESA and original source, disclose adaptations,
  noncommercial use only, and no implied ESA endorsement.

## 3. Stellarium sky-culture attribution

Space Zoo incorporates constellation and asterism names and line definitions
adapted from 42 Stellarium sky cultures from Stellarium v26.3, commit 3f23cf3a1a59523a9c7b1ec4540c33bca77addfa.
The source material was reformatted into Space Zoo runtime data. Sky-culture
artwork is not included.
See STELLARIUM_ATTRIBUTIONS.md for the authors, source locations, individual license terms, and modification details for each included sky culture.
Stellarium sky-culture material retains its applicable upstream license and is not relicensed under Space Zoo's project license.

## 4. SOFA and SatKit notices

- **IAU SOFA:** Space Zoo uses routines and computations derived by Hoyt
  Dynamic Laboratories from software provided by the Standards of Fundamental
  Astronomy project under license. Space Zoo is not SOFA-provided or
  SOFA-endorsed software. See `SOFA_DERIVED_WORK_NOTICE.md` for the full
  derived-work conditions, provenance and modifications.
  https://www.iausofa.org/terms-and-conditions
- **SatKit / Pluto numerical coefficients:** SatKit **0.24.1**, file
  `src/lpephem/planets.rs`; recorded numerical Pluto elements/rates and
  long-span correction-term provenance. No copied SatKit implementation code
  was identified in the reviewed builder. See `SATKIT_PROVENANCE_NOTICE.md`.
  https://docs.rs/satkit/0.24.1/src/satkit/lpephem/planets.rs.html

## 5. Rust runtime dependency notices

- The Linux/X11 target's 19 pinned Rust crates, individual copyrights and
  shared MIT terms: `THIRD_PARTY_NOTICES.md`.
- Rust standard library and exceptions, plus conditional native-library
  distribution treatment: `RUST_TOOLCHAIN_NOTICES.md`.

## 6. Public-domain and U.S. government image/source acknowledgments

The following credits are deliberately retained even where attribution is not
legally required. Their presence does not imply endorsement by NASA, JPL,
ESA, NASA mission partners, or other original creators.

### NASA / mission-source imagery

NASA-produced material is generally public domain in the United States,
subject to NASA media rules and any third-party rights shown at the source.
- NASA imagery-use guidelines: https://www.nasa.gov/nasa-brand-center/images-and-media/
- JPL image-use policy: https://www.jpl.nasa.gov/jpl-image-use-policy/

### Default Moon

- **Source:** NASA Scientific Visualization Studio, *Moon Phase and Libration, 2026*, https://svs.gsfc.nasa.gov/5587/
- **Space Zoo use:** source lunar imagery adapted into `assets/luna/nasa_64x144.l8`.
- **Credit:** NASA Scientific Visualization Studio; Lunar Reconnaissance Orbiter (LRO); visualizer Ernie Wright (USRA), planetary scientist Noah Petro (NASA/GSFC), producer James Tralie (eMITS), as identified by NASA SVS.
- **Use terms:** NASA media guidance as linked above; no NASA endorsement implied.

### Planet sprites

NASA- and NASA-mission-sourced imagery was adapted into Space Zoo's planet sprite artwork. The image-source acknowledgements are:

- **Mercury:** *Mercury Globe: 0°N, 0°E* — NASA/Johns Hopkins University Applied Physics Laboratory/Carnegie Institution of Washington. https://science.nasa.gov/photojournal/mercury-globe-0n-0e/
- **Venus:** *Venus from Mariner 10* — NASA/JPL-Caltech. https://science.nasa.gov/photojournal/venus-from-mariner-10/
- **Mars:** *Springtime on Mars* — Philip James (University of Toledo), Steven Lee (University of Colorado), NASA. https://science.nasa.gov/asset/hubble/springtime-on-mars/
- **Jupiter:** *Cassini Jupiter Portrait* — NASA/JPL/Space Science Institute. https://science.nasa.gov/resource/cassini-jupiter-portrait/
- **Saturn:** *Saturn in Color* — NASA/JPL/Space Science Institute. https://science.nasa.gov/photojournal/saturn-in-color/
- **Uranus:** *Uranus in True and False Color* — NASA/JPL. https://science.nasa.gov/photojournal/uranus-in-true-and-false-color/
- **Neptune:** *Neptune Full Disk View* — NASA/JPL. https://science.nasa.gov/photojournal/neptune-full-disk-view/
- **Pluto:** *Pluto in True Color* — NASA/Johns Hopkins University Applied Physics Laboratory/Southwest Research Institute. https://science.nasa.gov/photojournal/pluto-in-true-color/

- **Transformation:** imagery was cropped, resized, color/format transformed, and/or incorporated into project sprites for compact game display.
- **Use terms:** NASA's published media guidance and each source page's displayed contributor credits apply; the project does not imply endorsement by any named organization or contributor.

### Georges Méliès / GM-Moon

- **Historical source:** Georges Méliès, *Le Voyage dans la Lune* (*A Trip to the Moon*, 1902).
- **Status:** the original 1902 film imagery is in the public domain in the United States and France; this refers to the original historical work, not to any independent modern restoration contributions.
- **Space Zoo use:** historical imagery and design referenced for an optional stylized Moon in `assets/luna/melies_64x144.l8`.
- **Reference for original-work status:** https://commons.wikimedia.org/wiki/File:Le_Voyage_Dans_La_Lune.ogv

### Natural Earth ADMIN-1

The complete supplied DataHub attribution, Natural Earth terms, and PDDL text
are consolidated in the root `THIRD_PARTY_NOTICES.md`.

- **Source:** Natural Earth, *Admin-1 States and Provinces*, 1:10m cultural vectors, https://www.naturalearthdata.com/
- **Rights:** **public domain**, including adaptations and redistribution. Natural Earth explicitly does not require credit; the project provides acknowledgement for provenance. Terms: https://www.naturalearthdata.com/about/terms-of-use/
- **Acquisition/mirror:** DataHub `geo-ne-admin1`, https://datahub.io/core/geo-ne-admin1 — package marked **ODC-PDDL-1.0**, https://opendatacommons.org/licenses/pddl/1-0/ . Its separate distribution metadata and stated limitations for certain supplemental third-party attribute columns should not be confused with the underlying Natural Earth public-domain map geometry.
- **Space Zoo use:** administrative names/codes, bounding boxes, and polygon geometry normalized into a compact ADMIN-1 runtime table; project-developed water and fictional-geometry rows are separate adaptations/additions.
- **Source identity detail:** the particular upstream revision and raw source-input checksum are historical data-reproducibility metadata, not an outstanding Natural Earth redistribution permission.

### Null Island and Île Lincoln

- **Null Island:** the fictional label/location at latitude 0°, longitude 0° is a project/game-geography convention, not a third-party illustrated source.
- **Île Lincoln historical source:** map of *Île Lincoln* in Jules Verne's *L'Île mystérieuse* (*The Mysterious Island*), original Hetzel-edition publication **1875**; author/artist credits on Wikimedia Commons: **Jules Verne, Jules Férat, Charles Barbant**.
- **Source image and rights statement:** *File:Ile Mysterieuse 03.jpg*, https://commons.wikimedia.org/wiki/File:Ile_Mysterieuse_03.jpg — Wikimedia Commons designates the historical map **public domain** in its country of origin and the United States, with Creative Commons **Public Domain Mark 1.0** (https://creativecommons.org/publicdomain/mark/1.0/).

### Howard Hinnant date algorithms

- **Source:** https://howardhinnant.github.io/date_algorithms.html
- **Recorded rights status:** the documented algorithms are dedicated by the author to the public domain.
- **Space Zoo use:** civil-date conversion arithmetic in `src/astronomy.rs` and `src/time_machine.rs`.

## 7. Additional source-build acknowledgements

### IERS / Time Standards

- **Source:** IERS Bulletin C, through the source version identified in code as Bulletin C 72 (2026-07-06).
- **Space Zoo use:** factual UTC/leap-second transition values used by the stellar time model.
- **Rights status:** Space Zoo encodes factual time-standard values and does not reproduce Bulletin C prose, formatting, or software.

### ERFA / PyERFA / NumPy

- **ERFA / PyERFA 2.0.1.5:** independent offline reference engine used to generate validation fixtures; **BSD-3-Clause**; not a runtime dependency and not production astrometry code.
- **NumPy 2.3.5:** offline dependency used with the PyERFA fixture generator; **BSD-3-Clause**; not a runtime dependency.

### NOAA Solar Calculations

- **Source:** NOAA solar-calculation equations and explanatory material.
- **Space Zoo use:** mathematical/reference basis for the lightweight solar-position, sunrise/sunset, and twilight implementation in `src/astronomy.rs`.
- **Rights status:** implemented as Space Zoo code from published scientific equations; no NOAA source code or artwork is copied.

### Meeus / VSOP / IAU Models

- **Jean Meeus, _Astronomical Algorithms_, 2nd ed.:** mathematical/reference source for the lunar periodic model and portions of solar/nutation approximations; Space Zoo contains its own implementation and does not reproduce book prose or figures.
- **VSOP-derived solar approximations / IAU 2006 and IAU 2000 model terms:** scientific model lineage and numerical reference material; credited as scientific sources rather than bundled third-party software.

## . Inspiration

- **btop — Jakob P. Liljenberg (Aristocratos):** sick system monitor.
- **GCompris — Bruno Coudoin and the GCompris/KDE contributors:** spiritual inspiration, found while looking for computer games suitable for developing young minds.