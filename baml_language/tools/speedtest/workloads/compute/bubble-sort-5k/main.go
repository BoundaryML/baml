package main

func Main() int {
	arr := []int{}
	for i := 0; i < 5000; i++ {
		arr = append(arr, 5000-i)
	}
	for i := 0; i < len(arr); i++ {
		for j := 0; j < len(arr)-i-1; j++ {
			if arr[j] > arr[j+1] {
				tmp := arr[j]
				arr[j] = arr[j+1]
				arr[j+1] = tmp
			}
		}
	}
	return arr[0] + arr[4999]
}
