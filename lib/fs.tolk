/// Namespace for file system operations.
struct fs

/// Reads the entire contents of a file at the given path as a string slice
/// with snake-string encoding.
///
/// Returns the file contents as a slice if the file exists and can be read,
/// or `null` if the file doesn't exist or cannot be read.
///
/// Example:
/// ```tolk
/// val content = fs.readFile("config.txt");
/// if (content != null) {
///     println1("File content: {}", content);
/// } else {
///     println("File not found");
/// }
/// ```
fun fs.readFile(path: slice): slice? asm "3 EXTCALL"
