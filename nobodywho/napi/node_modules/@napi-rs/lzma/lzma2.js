const { lzma2 } = require('./index')

module.exports.compress = lzma2.compress
module.exports.compressSync = lzma2.compressSync
module.exports.decompress = lzma2.decompress
module.exports.decompressSync = lzma2.decompressSync
