#!/usr/bin/env python3
"""Apply narrowly guarded RWS patches to pinned SSHFS 3.7.5 source."""
from pathlib import Path
import sys

path = Path(sys.argv[1])
source = path.read_text()

def replace_once(old, new):
    global source
    if source.count(old) != 1:
        raise SystemExit(f"Unexpected SSHFS source, refusing patch: {old[:70]!r}")
    source = source.replace(old, new)

replace_once("\t/* Readahead should be done by kernel or sshfs but not both */",
             "#ifdef __APPLE__\n\tconn->want_darwin &= ~FUSE_DARWIN_CAP_RENAME_EXT;\n#endif\n\t/* Readahead should be done by kernel or sshfs but not both */")
replace_once("\tint rename_workaround;", "\tint rename_workaround;\n\tint rws_unicode;")
replace_once('\tSSHFS_OPT("sync_readdir",      sync_readdir, 1),',
             '\tSSHFS_OPT("sync_readdir",      sync_readdir, 1),\n\tSSHFS_OPT("rws_unicode",       rws_unicode, 1),')
replace_once("static inline void buf_add_path(struct buffer *buf, const char *path)\n{\n\tchar *realpath;",
'''/* NFC remote names, NFD local presentation; no mutation of existing remote names.
 * GLib handles supplementary-plane characters unlike this macOS UTF-8-MAC iconv.
 * Raw names remain available by omitting -o rws_unicode. */
static char *rws_normalize(const char *path, GNormalizeMode mode)
{
	char *normalized = sshfs.rws_unicode ? g_utf8_normalize(path, -1, mode) : NULL;
	return normalized ? normalized : g_strdup(path);
}

static inline void buf_add_path(struct buffer *buf, const char *path)
{
	char *realpath;
	char *normalized = rws_normalize(path, G_NORMALIZE_NFC);
	path = normalized;''')
replace_once("\tbuf_add_string(buf, realpath);\n\tg_free(realpath);",
             "\tbuf_add_string(buf, realpath);\n\tg_free(realpath);\n\tg_free(normalized);")
replace_once("\t\t\t\tfiller(dbuf, name, &stbuf, 0, 0);",
             "\t\t\t\tchar *local_name = rws_normalize(name, G_NORMALIZE_NFD);\n\t\t\t\tfiller(dbuf, local_name, &stbuf, 0, 0);\n\t\t\t\tg_free(local_name);")
replace_once("\t\t\tstrncpy(linkbuf, link, size - 1);",
             "\t\t\tchar *local_link = rws_normalize(link, G_NORMALIZE_NFD);\n\t\t\tstrncpy(linkbuf, local_link, size - 1);\n\t\t\tg_free(local_link);")
replace_once("\tbuf_add_string(&buf, from);\n\tbuf_add_path(&buf, to);",
             "\tchar *remote_link = rws_normalize(from, G_NORMALIZE_NFC);\n\tbuf_add_string(&buf, remote_link);\n\tg_free(remote_link);\n\tbuf_add_path(&buf, to);")
# SSHFS caches use byte-exact path keys; canonical aliases must not retain
# independent entries. FSKit attribute/entry caches are also disabled for this mode.
replace_once("\tif(sshfs.dir_cache)\n\t\tsshfs.op = cache_wrap(&sshfs_oper);",
             "\tif (sshfs.rws_unicode)\n\t\tsshfs.dir_cache = 0;\n\tif(sshfs.dir_cache)\n\t\tsshfs.op = cache_wrap(&sshfs_oper);")
replace_once("\t// SFTP only supports 1-second time resolution",
             "\tif (sshfs.rws_unicode) {\n\t\tcfg->attr_timeout = 0;\n\t\tcfg->entry_timeout = 0;\n\t\tcfg->negative_timeout = 0;\n\t}\n\n\t// SFTP only supports 1-second time resolution")

path.write_text(source)
