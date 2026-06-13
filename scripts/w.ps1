# Special argument handling.
if ($args.Count -eq 1) {
  if ($args[0] -eq "-") {
    # Previous directory.
    popd
    return;
  }
  if ($args[0] -eq "--help") {
    # Help printing.
    warpto --help
    return;
  }
}

$output = warpto @args

if ($LASTEXITCODE -ne 0) {
    echo $output
    return
}

pushd $output
