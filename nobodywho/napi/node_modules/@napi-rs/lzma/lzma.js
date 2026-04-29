const { lzma } = require('./index')

module.exports.compress = lzma.compress
module.exports.compressSync = lzma.compressSync
module.exports.decompress = lzma.decompress
module.exports.decompressSync = lzma.decompressSync
