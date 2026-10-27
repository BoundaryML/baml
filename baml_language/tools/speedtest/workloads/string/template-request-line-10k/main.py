# The request line an HTTP client builds, as one backtick template with
# string and int holes, 10,000 times.
def main():
    names = ["apple", "pear", "plum", "fig", "kiwi"]
    families = ["FR", "VG", "DY"]
    limit = 25
    host = "api.example.com"
    seed = 7
    total = 0
    for i in range(10000):
        seed = seed * 16807 % 2147483647
        org = families[i % 3]
        item = names[seed % 5]
        id = seed % 100000
        page = i % 50
        s = f"GET /v1/orgs/{org}/items/{item}/{id}?page={page}&limit={limit}&sort={item}&order={org} host={host} user={item}-{id} attempt={page} region={org}"
        total += len(s)
    return total
