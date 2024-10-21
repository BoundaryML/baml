To run the test with a filter:

``` bash
pnpm integ-tests -t "works with fallbacks"
```


Note: Before running, you need to build the typescript runtime:

``` bash
cd engine/language_client_typescript
pnpm build:debug
```
