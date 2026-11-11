Attribute VB_Name = "WebHelpersExcerpt"
' Verbatim excerpt of VBA-Web v4.1.6 src/WebHelpers.bas (c) Tim Hall, MIT licence,
' https://github.com/VBA-tools/VBA-Web commit 9dbcc751d177099f20c96c5ee332ec10ef47423c.
' Only whole enums and functions are copied, unchanged and in source order; the module header,
' declares, the other sections and every comment between them are left out. See ../LICENSE.
Option Explicit

Public Enum WebMethod
    HttpGet = 0
    HttpPost = 1
    HttpPut = 2
    HttpDelete = 3
    HttpPatch = 4
    HttpHead = 5
End Enum

Public Enum UrlEncodingMode
    StrictUrlEncoding
    FormUrlEncoding
    QueryUrlEncoding
    CookieUrlEncoding
    PathUrlEncoding
End Enum

Public Sub LogWarning(Message As String, Optional From As String = "VBA-Web")
    Debug.Print "WARNING - " & From & ": " & Message
End Sub

Public Function Obfuscate(Secure As String, Optional Character As String = "*") As String
    Obfuscate = VBA.String$(VBA.Len(Secure), Character)
End Function

Public Function UrlEncode(Text As Variant, _
    Optional SpaceAsPlus As Boolean = False, Optional EncodeUnsafe As Boolean = True, _
    Optional EncodingMode As UrlEncodingMode = UrlEncodingMode.StrictUrlEncoding) As String

    If SpaceAsPlus = True Then
        LogWarning "SpaceAsPlus is deprecated and will be removed in VBA-Web v5. " & _
            "Use EncodingMode:=FormUrlEncoding instead", "WebHelpers.UrlEncode"
    End If
    If EncodeUnsafe = False Then
        LogWarning "EncodeUnsafe has been removed as it was based on an outdated url encoding specification. " & _
            "Use EncodingMode:=CookieUrlEncoding to approximate this behavior", "WebHelpers.UrlEncode"
    End If

    Dim web_UrlVal As String
    Dim web_StringLen As Long

    web_UrlVal = VBA.CStr(Text)
    web_StringLen = VBA.Len(web_UrlVal)

    If web_StringLen > 0 Then
        Dim web_Result() As String
        Dim web_i As Long
        Dim web_CharCode As Integer
        Dim web_Char As String
        Dim web_Space As String
        ReDim web_Result(web_StringLen)

        ' StrictUrlEncoding - ALPHA / DIGIT / "-" / "." / "_" / "~"
        ' FormUrlEncoding   - ALPHA / DIGIT / "-" / "." / "_" / "*" / (space) -> "+"
        ' QueryUrlEncoding  - ALPHA / DIGIT / "-" / "." / "_"
        ' CookieUrlEncoding - strict / "!" / "#" / "$" / "&" / "'" / "(" / ")" / "*" / "+" /
        '   "/" / ":" / "<" / "=" / ">" / "?" / "@" / "[" / "]" / "^" / "`" / "{" / "|" / "}"
        ' PathUrlEncoding   - strict / "!" / "$" / "&" / "'" / "(" / ")" / "*" / "+" / "," / ";" / "=" / ":" / "@"

        ' Set space value
        If SpaceAsPlus Or EncodingMode = UrlEncodingMode.FormUrlEncoding Then
            web_Space = "+"
        Else
            web_Space = "%20"
        End If

        ' Loop through string characters
        For web_i = 1 To web_StringLen
            ' Get character and ascii code
            web_Char = VBA.Mid$(web_UrlVal, web_i, 1)
            web_CharCode = VBA.Asc(web_Char)

            Select Case web_CharCode
                Case 65 To 90, 97 To 122
                    ' ALPHA
                    web_Result(web_i) = web_Char
                Case 48 To 57
                    ' DIGIT
                    web_Result(web_i) = web_Char
                Case 45, 46, 95
                    ' "-" / "." / "_"
                    web_Result(web_i) = web_Char

                Case 32
                    ' (space)
                    ' FormUrlEncoding -> "+"
                    ' Else -> "%20"
                    web_Result(web_i) = web_Space

                Case 33, 36, 38, 39, 40, 41, 43, 58, 61, 64
                    ' "!" / "$" / "&" / "'" / "(" / ")" / "+" / ":" / "=" / "@"
                    ' PathUrlEncoding, CookieUrlEncoding -> Unencoded
                    ' Else -> Percent-encoded
                    If EncodingMode = UrlEncodingMode.PathUrlEncoding Or EncodingMode = UrlEncodingMode.CookieUrlEncoding Then
                        web_Result(web_i) = web_Char
                    Else
                        web_Result(web_i) = "%" & VBA.Hex(web_CharCode)
                    End If

                Case 35, 45, 46, 47, 60, 62, 63, 91, 93, 94, 95, 96, 123, 124, 125
                    ' "#" / "-" / "." / "/" / "<" / ">" / "?" / "[" / "]" / "^" / "_" / "`" / "{" / "|" / "}"
                    ' CookieUrlEncoding -> Unencoded
                    ' Else -> Percent-encoded
                    If EncodingMode = UrlEncodingMode.CookieUrlEncoding Then
                        web_Result(web_i) = web_Char
                    Else
                        web_Result(web_i) = "%" & VBA.Hex(web_CharCode)
                    End If

                Case 42
                    ' "*"
                    ' FormUrlEncoding, PathUrlEncoding, CookieUrlEncoding -> "*"
                    ' Else -> "%2A"
                    If EncodingMode = UrlEncodingMode.FormUrlEncoding _
                        Or EncodingMode = UrlEncodingMode.PathUrlEncoding _
                        Or EncodingMode = UrlEncodingMode.CookieUrlEncoding Then

                        web_Result(web_i) = web_Char
                    Else
                        web_Result(web_i) = "%" & VBA.Hex(web_CharCode)
                    End If

                Case 44, 59
                    ' "," / ";"
                    ' PathUrlEncoding -> Unencoded
                    ' Else -> Percent-encoded
                    If EncodingMode = UrlEncodingMode.PathUrlEncoding Then
                        web_Result(web_i) = web_Char
                    Else
                        web_Result(web_i) = "%" & VBA.Hex(web_CharCode)
                    End If

                Case 126
                    ' "~"
                    ' FormUrlEncoding, QueryUrlEncoding -> "%7E"
                    ' Else -> "~"
                    If EncodingMode = UrlEncodingMode.FormUrlEncoding Or EncodingMode = UrlEncodingMode.QueryUrlEncoding Then
                        web_Result(web_i) = "%7E"
                    Else
                        web_Result(web_i) = web_Char
                    End If

                Case 0 To 15
                    web_Result(web_i) = "%0" & VBA.Hex(web_CharCode)
                Case Else
                    web_Result(web_i) = "%" & VBA.Hex(web_CharCode)

                ' TODO For non-ASCII characters,
                '
                ' FormUrlEncoded:
                '
                ' Replace the character by a string consisting of a U+0026 AMPERSAND character (&), a "#" (U+0023) character,
                ' one or more ASCII digits representing the Unicode code point of the character in base ten, and finally a ";" (U+003B) character.
                '
                ' Else:
                '
                ' Encode to sequence of 2 or 3 bytes in UTF-8, then percent encode
                ' Reference Implementation: https://www.w3.org/International/URLUTF8Encoder.java
            End Select
        Next web_i
        UrlEncode = VBA.Join$(web_Result, "")
    End If
End Function

Public Function UrlDecode(Encoded As String, _
    Optional PlusAsSpace As Boolean = True, _
    Optional EncodingMode As UrlEncodingMode = UrlEncodingMode.StrictUrlEncoding) As String

    Dim web_StringLen As Long
    web_StringLen = VBA.Len(Encoded)

    If web_StringLen > 0 Then
        Dim web_i As Long
        Dim web_Result As String
        Dim web_Temp As String

        For web_i = 1 To web_StringLen
            web_Temp = VBA.Mid$(Encoded, web_i, 1)

            If web_Temp = "+" And _
                (PlusAsSpace _
                 Or EncodingMode = UrlEncodingMode.FormUrlEncoding _
                 Or EncodingMode = UrlEncodingMode.QueryUrlEncoding) Then

                web_Temp = " "
            ElseIf web_Temp = "%" And web_StringLen >= web_i + 2 Then
                web_Temp = VBA.Mid$(Encoded, web_i + 1, 2)
                web_Temp = VBA.Chr(VBA.CInt("&H" & web_Temp))

                web_i = web_i + 2
            End If

            ' TODO Handle non-ASCII characters

            web_Result = web_Result & web_Temp
        Next web_i

        UrlDecode = web_Result
    End If
End Function

Public Function JoinUrl(LeftSide As String, RightSide As String) As String
    If Left(RightSide, 1) = "/" Then
        RightSide = Right(RightSide, Len(RightSide) - 1)
    End If
    If Right(LeftSide, 1) = "/" Then
        LeftSide = Left(LeftSide, Len(LeftSide) - 1)
    End If

    If LeftSide <> "" And RightSide <> "" Then
        JoinUrl = LeftSide & "/" & RightSide
    Else
        JoinUrl = LeftSide & RightSide
    End If
End Function

Public Function MethodToName(Method As WebMethod) As String
    Select Case Method
    Case WebMethod.HttpDelete
        MethodToName = "DELETE"
    Case WebMethod.HttpPut
        MethodToName = "PUT"
    Case WebMethod.HttpPatch
        MethodToName = "PATCH"
    Case WebMethod.HttpPost
        MethodToName = "POST"
    Case WebMethod.HttpGet
        MethodToName = "GET"
    Case WebMethod.HttpHead
        MethodToName = "HEAD"
    End Select
End Function
