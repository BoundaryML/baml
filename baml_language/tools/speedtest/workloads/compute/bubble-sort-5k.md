# compute::bubble sort 5k

## BAML
```baml
function main() -> int {
  let arr: int[] = [];
  for (let i = 0; i < 5000; i += 1) { arr.push(5000 - i); };
  for (let i = 0; i < arr.length(); i += 1) {
    for (let j = 0; j < arr.length() - i - 1; j += 1) {
      if arr[j] > arr[j + 1] {
        let tmp = arr[j];
        arr[j] = arr[j + 1];
        arr[j + 1] = tmp;
      };
    };
  };
  return arr[0] + arr[4999];
}
```

## Python
```python
arr = list(range(5000, 0, -1))
n = len(arr)
for i in range(n):
    for j in range(n - i - 1):
        if arr[j] > arr[j+1]:
            arr[j], arr[j+1] = arr[j+1], arr[j]
print(arr[0] + arr[4999])
```

## Typescript
```ts
const arr=[];for(let i=0;i<5000;i++)arr.push(5000-i);
const n=arr.length;for(let i=0;i<n;i++)for(let j=0;j<n-i-1;j++)if(arr[j]>arr[j+1]){const t=arr[j];arr[j]=arr[j+1];arr[j+1]=t}
console.log(arr[0]+arr[4999])
```
