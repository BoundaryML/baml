# Pure BAML malloc pressure-relief experiment at 4,000 RPS

This run served 1,200,000/1,200,000 HTTP requests successfully during one continuous 300-second load. RSS rose from 37.1 MiB to 159.1 MiB and malloc reservation rose from 44 MiB to 164 MiB, then both were exactly flat for the final 90 loaded seconds.

The explicit major GC reduced runtime objects from 204,133 to 15, malloc blocks from 276,274 to 129,476, and live malloc bytes from 50.4 MiB to 19.2 MiB. It did not reduce the 164 MiB malloc reservation or 159.1 MiB RSS.

The post-GC `vmmap` snapshot shows why this is not a live-allocation leak. The default malloc zone retained 165.3 MiB of virtual space and 143.7 MiB appeared resident, but only 24.7 MiB was dirty. The zone contained 76 MiB of completely empty `MALLOC_SMALL` regions; 65.2 MiB appeared resident but only 0.83 MiB was dirty. The dirty, nonempty zone held 19.5 MiB allocated plus 5.2 MiB internal fragmentation.

Calling `malloc_zone_pressure_relief(NULL, 0)` requested maximal relief from every malloc zone. It returned zero bytes, RSS changed by +0.02 MiB immediately and +0.08 MiB after 30 seconds, and the process remained healthy. The allocator therefore had no additional dirty/releasable pages to purge: most of the gap was already-clean reusable memory that remained mapped and visible in RSS.

Classification: allocator high-water retention of virtual address space and clean/reusable pages, with about 5.2 MiB of bounded small-object fragmentation. This is not a classical memory leak of unreachable live allocations. The original run's late reservation step did not recur here, so this five-minute repetition still cannot prove a permanent upper bound.

See [analysis.json](analysis.json), [summary.json](summary.json), [vmmap before GC](vmmap-before-gc.txt), [vmmap after GC](vmmap-after-gc.txt), and [vmmap after pressure relief](vmmap-after-relief.txt).
