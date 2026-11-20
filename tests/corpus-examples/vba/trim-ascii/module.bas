Attribute VB_Name = "Dc_trim_ascii_6f1146c"
Option Explicit

Public Function DcExample() As Variant
    DcExample = Array("[" & Trim("  a  b  ") & "]", AscW(Left(Trim(vbTab & " a "), 1)))
End Function
