This is contour 0.13.1 from crates.io, licensed under MIT or Apache-2.0;
the upstream license texts are included alongside this file. The local patch
adds a cancellable isoline API and checks during marching squares and ring
processing. The existing APIs retain their behavior. The application's test
suite compares the patched output with the unmodified crates.io release.
