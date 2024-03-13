'use strict';

var fs     =  require('fs');
var path   =  require('path');
var xtend  =  require('xtend');
var crypto =  require('crypto');

var loadeds = {};
var extensions = xtend(require.extensions);
 
function getHash(data) {
  return crypto
    .createHash('md5')
    .update(data)
    .digest('hex');
}

/**
 * Activates deduping for files with the given extension.
 * 
 * @name activate
 * @function
 * @param ext {String} (optional) extension for which to activate deduping (default: '.js')
 * @param subdirs {Number} (optional) how many subdirs right above the module
 *    have to be the same in order for it to be considered identical  (default: 2)
 *
 *  Example: sudirs: 2 -- x/foo/bar/main.js === y/foo/bar/main.js
 *                        x/boo/bar/main.js !== y/foo/bar/main.js
 */
exports.activate = function (ext, subdirs) { 
  ext = ext || '.js';
  subdirs = typeof subdirs === 'undefined' ? 2 : subdirs;

  var ext_super = require.extensions[ext];

  require.extensions[ext] = function dedupingExtension(module, file) {

    var src = fs.readFileSync(file, 'utf8');

    // hash includes filename and subdir name(s) to make override more strict
    var fulldir  =  path.dirname(file);
    var dirs     =  fulldir.split(path.sep);
    var dir      =  '';

    for (var i = subdirs; i > 0 && dirs.length; i--) dir = dirs.pop() + dir;

    var filename =  path.basename(file);
    var hash     =  getHash(src + dir + filename);

    var loaded = loadeds[hash];
    if (loaded) {
      module.exports = loaded.module.exports;
    } else {
      ext_super(module, file);
      loadeds[hash] = { file: file,  module: module };
    }
  };
};

/**
 * Deactivates deduping files with the given extension.
 * 
 * @name deactivate
 * @function
 * @param ext {String} (optional) extension for which to activate deduping (default: '.js')
 */
exports.deactivate = function (ext) {
  ext = ext || '.js';
  require.extensions[ext] = extensions[ext];
};

/**
 * Clears the registry that contains previously loaded modules.
 * 
 * @name reset
 * @function
 */
exports.reset = function () {
  loadeds = {};
};
