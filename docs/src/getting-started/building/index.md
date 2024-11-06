# Building

The following chapters will cover how to build the Polka Storage parachain using multiple methods.

<div class="warning">
Quick reminder that Windows is not part of the supported operating systems.
As such, the following guides <b>have not been tested</b> on Windows.
</div>

We provide three main methods, each with their own advantages and disadvantages:

* Building natively — less convenient but binaries are portable across machines
* Building with `nix` — easy to build but binaries are not portable across machines[^note]
* Building with `docker` — convenient but slow, plus the result will be Docker images instead of binaries

[^note]: We're using `nix` mainly for development, as such we didn't fine tune it for building production artifacts.
