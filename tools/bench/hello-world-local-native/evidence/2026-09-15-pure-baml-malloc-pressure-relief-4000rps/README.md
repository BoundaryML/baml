# Pure BAML malloc pressure-relief experiment at 4,000 RPS

This corrected run served 1,200,000/1,200,000 HTTP requests successfully during one continuous 300-second load. RSS rose from 37.2 MiB to 160.3 MiB and malloc reservation rose from 48 MiB to 160 MiB. It reproduced a late reservation step from 156 MiB to 160 MiB at 202 seconds while the live-byte sawtooth continued to reclaim; RSS and reservation were exactly flat for the final 60 loaded seconds.

The explicit major GC reduced runtime objects from 204,133 to 15, malloc blocks from 276,270 to 129,472, and live malloc bytes from 50.4 MiB to 19.2 MiB. It did not reduce the 160 MiB malloc reservation or 160.3 MiB RSS.

The post-GC `vmmap` snapshot shows why this is not a live-allocation leak. The default malloc zone retained 161.3 MiB of virtual space and 144.9 MiB appeared resident, but only 24.9 MiB was dirty. The zone contained 76 MiB of completely empty `MALLOC_SMALL` regions; 72.2 MiB appeared resident but only 0.25 MiB was dirty. The dirty, nonempty zone held 19.4 MiB allocated plus 5.5 MiB internal fragmentation.

Calling `malloc_zone_pressure_relief(NULL, 0)` requested maximal relief from every malloc zone. It returned zero bytes, RSS changed by one 16 KiB page immediately and remained there after 30 seconds, and the process remained healthy. No HTTP or heap-stat request ran between the explicit GC and final RSS point. This establishes that the call caused no additional unmapping. It does not prove that every clean page was immediately kernel-reclaimable or rule out future address-space growth; the separate `vmmap` measurements show that clean pages, rather than live allocations or dirty fragmentation, dominate the apparent RSS gap.

Classification: allocator high-water retention of virtual address space and clean/reusable pages, with about 5.5 MiB of bounded small-object fragmentation. This is not a classical memory leak of unreachable live allocations. The original late reservation behavior did recur, although this five-minute repetition still cannot prove a permanent upper bound.

See [analysis.json](analysis.json), [summary.json](summary.json), [vmmap before GC](vmmap-before-gc.txt), [vmmap after GC](vmmap-after-gc.txt), and [vmmap after pressure relief](vmmap-after-relief.txt).
