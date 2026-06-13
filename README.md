# warp-to

```
warp-to: A portal to nearby filesystems.

Usage: [1] w [OPTIONS] <TERMS>...
       [2] w .<N>
       [3] w +<SHORTCUT>

* [1] Term-based search.
 Search nearby folders to the CWD for a folder matching the given set of terms.
 `warp-to` will search both higher and lower heirarchies for the first term, and
 lower heirarchies only for following terms.

 For example, the below will search for a directory named "foo", and then try to
 find a child directory somewhere named "bar".
   ~> w foo bar
 You can also chain as many terms together as you like.
   ~> w foo bar fugu hoge

 If you know a concrete directory structure that you want to search for, you can
 do so using slashes as delimiters. For example, the below would search for a
 concrete directory structure "foo/bar/fugu", followed by a child directory
 somewhere named "hoge".
   ~> w foo/bar/fugu hoge

 The special path characters you are used to on Unix-like systems also work with
 `warp-to`, such as root ("/") and home ("~").
   ~> w / hoge
   ~> w ~ doom

 In addition to system directories, you can also search for child directories of
 user-configured shortcuts, seen in [3]. The below would first resolve the
 shortcut "foobar", and then search for some child directory named "doom".
   ~> w +foobar doom

* [2] Jump-based navigation.
 Jump directly to a parent directory, with the number of hops determined by the
 "N" parameter. For example, the below command will jump 4 directories up from
 the CWD. You can specify from 1 (or ..) up to 9.
   ~> w .4

 You can also use relative jumps as the start of a term-based search.
   ~> w .4 foo
   ~> w .. bar

* [3] Shortcut-based navigation.
 Jump directly to a user specified shortcut directory. These are specified in
 the `warp-to` settings file, found at either `$CONFIG/warp-to` on Unix-like
 systems, or `%APPDATA%/warp-to` on Windows.
   ~> w +foobar

Options:
  -d, --distance <DIST>  The maximum distance of the search, default 5.
                         Increasing this value will extend the search time of
                         the program, but provide better results.

Terms:
  *  /              The root directory.
                    On Windows, the drive root relative to the CWD.
  *  ~              The current user's home directory.
  *  <PATH>         A concrete path name.
  *  .<N>           A relative jump up from the CWD of N spaces.
                    Also supports "..".
  *  +<SHORTCUT>    A user-defined shortcut. See [3] for details.
```
