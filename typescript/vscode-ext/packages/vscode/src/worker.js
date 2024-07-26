onmessage = (event) => {
  console.log('Message received from main script')
  console.log('Received from worker:', event.data)
  fetch('http://localhost:8081')
    .then((response) => response.text())
    .then((data) => {
      console.log('Data fetched from localhost:8081:')
      console.log(data)
    })
    .catch((error) => {
      console.error('Error fetching data:', error)
    })
}
