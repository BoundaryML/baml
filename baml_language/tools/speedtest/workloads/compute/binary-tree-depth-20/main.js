function t(d){return d<=0?1:t(d-1)+t(d-1)}
function main() {
  return t(20);
}
