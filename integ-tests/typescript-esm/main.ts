import { b } from "./baml_client/index.js";
// import { Image} from "@boundaryml/baml";
import pkg from "@boundaryml/baml";
// Force another import for the logging path.
import  { setLogLevel } from "./baml_client/config.js";

const { Image } = pkg;

setLogLevel("info");

const result = await b.DescribeImage( Image.fromUrl('https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png'));

console.log(result);
