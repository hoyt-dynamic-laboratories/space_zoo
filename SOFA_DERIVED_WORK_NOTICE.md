# IAU SOFA Derived-Work Notice — Space Zoo

Space Zoo uses routines and computations derived by Hoyt Dynamic Laboratories from software provided by the International Astronomical Union Standards of Fundamental Astronomy (IAU SOFA) project under the SOFA Software License. Space Zoo does not itself constitute software provided by or endorsed by SOFA.

`src/stellar/astrometry.rs` adapts SOFA's `fw2m` rotation composition.
Changes include a Rust implementation, project-specific types and routine
names, an active column-vector rotation convention, and truncated nutation.

Copyright: Standards Of Fundamental Astronomy Board of the International Astronomical Union.

SOFA Software License: https://www.iausofa.org/terms-and-conditions

## Derived-work conditions

a) Your work shall be marked or carry a statement that it (i) uses routines and computations derived by you from software provided by SOFA under license to you; and (ii) does not itself constitute software provided by and/or endorsed by SOFA.

b) The source code of your derived work must contain descriptions of how the derived work is based upon, contains and/or differs from the original SOFA software.

c) The names of all routines in your derived work shall not include the prefix "iau" or "sofa" or trivial modifications thereof such as changes of case.

d) The origin of the SOFA components of your derived work must not be misrepresented; you must not claim that you wrote the original software, nor file a patent application for SOFA software or algorithms embedded in the SOFA software.

e) These requirements must be reproduced intact in any source distribution and shall apply to anyone to whom you have granted a further right to modify the source code of your derived work.
