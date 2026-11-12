# Circular Reference: an original DNA Calc example, authored 2026-11-12. This section is
# called by make-catalogue-fixtures.ps1 and uses its deterministic package writer.
function Write-CircularReferenceGame([string]$Path) {
    $initial = '0000000P089|088|087|086'
    # U1 is a single, self-referencing state: tick (4), food count (3), outcome (1),
    # food position (3), then the head-first body as delimited three-digit positions. One state cell makes
    # a tick atomic without relying on the order of several circular cells.
    $state = @'
_xlfn.LET(_xlpm.old,IF(LEN(U1)<23,"0000000P089|088|087|086",U1),_xlpm.tick,VALUE(LEFT(_xlpm.old,4)),_xlpm.eaten,VALUE(MID(_xlpm.old,5,3)),_xlpm.result,MID(_xlpm.old,8,1),_xlpm.body,MID(_xlpm.old,12,999),_xlpm.head,VALUE(MID(_xlpm.body,2,3)),_xlpm.col,MOD(_xlpm.head-1,16)+1,_xlpm.row,INT((_xlpm.head-1)/16)+1,_xlpm.dx,IF(B2="R",1,IF(B2="L",-1,0)),_xlpm.dy,IF(B2="D",1,IF(B2="U",-1,0)),_xlpm.next,_xlpm.head+_xlpm.dx+16*_xlpm.dy,_xlpm.food,VALUE(MID(_xlpm.old,9,3)),_xlpm.grow,_xlpm.next=_xlpm.food,_xlpm.wall,OR(_xlpm.col+_xlpm.dx<1,_xlpm.col+_xlpm.dx>16,_xlpm.row+_xlpm.dy<1,_xlpm.row+_xlpm.dy>12),_xlpm.hit,ISNUMBER(SEARCH("|"&RIGHT("000"&_xlpm.next,3),IF(_xlpm.grow,_xlpm.body,LEFT(_xlpm.body,LEN(_xlpm.body)-4)))),_xlpm.end,IF(_xlpm.wall,"R",IF(_xlpm.next=75,"D",IF(_xlpm.hit,"C","P"))),IF(B3=1,"0000000P089|088|087|086",IF(OR(_xlpm.result<>"P",AND(_xlpm.dx=0,_xlpm.dy=0)),_xlpm.old,RIGHT("0000"&(_xlpm.tick+1),4)&RIGHT("000"&(_xlpm.eaten+IF(AND(_xlpm.grow,_xlpm.end="P"),1,0)),3)&_xlpm.end&RIGHT("000"&IF(AND(_xlpm.grow,_xlpm.end="P"),IF(_xlpm.eaten=0,90,IF(_xlpm.eaten=1,91,IF(MOD((_xlpm.tick+1)*37+37,192)+1=75,76,MOD((_xlpm.tick+1)*37+37,192)+1))),_xlpm.food),3)&IF(_xlpm.end="P","|"&RIGHT("000"&_xlpm.next,3)&IF(_xlpm.grow,_xlpm.body,LEFT(_xlpm.body,LEN(_xlpm.body)-4)),_xlpm.body))))
'@
    $cells = @{}
    function Add-GameCell([int]$Row, [string]$Column, [string]$Xml) {
        if (-not $cells.ContainsKey($Row)) { $cells[$Row] = @{} }
        $cells[$Row][$Column] = $Xml
    }
    function Game-Text([int]$Row, [string]$Column, [string]$Value) {
        $ref = "$Column$Row"
        Add-GameCell $Row $Column ('<c r="' + $ref + '" t="inlineStr"><is><t>' + (Get-Escaped $Value) + '</t></is></c>')
    }
    function Game-Formula([int]$Row, [string]$Column, [string]$Formula, [string]$Extra = '') {
        $ref = "$Column$Row"
        Add-GameCell $Row $Column ('<c r="' + $ref + '"' + $Extra + '><f>' + (Get-Escaped $Formula) + '</f></c>')
    }
    Game-Text 1 'B' 'Circular Reference: a spreadsheet snake'
    Game-Text 2 'A' 'Direction'; Game-Text 3 'A' 'Reset'; Game-Text 3 'C' 'Set 1; clear direction; set 0'
    Add-GameCell 3 'B' '<c r="B3"><v>0</v></c>'
    Game-Text 4 'A' 'Status'; Game-Text 5 'A' 'SUM'; Game-Text 6 'A' 'Length'
    # This formula needs an explicit text seed.
    $cells[1]['U'] = '<c r="U1" t="str"><f>' + (Get-Escaped $state) + '</f><v>' + $initial + '</v></c>'
    Game-Formula 4 'B' 'IF(MID(U1,8,1)="P","Playing",IF(MID(U1,8,1)="R","GAME OVER: #REF!",IF(MID(U1,8,1)="D","GAME OVER: #DIV/0!","GAME OVER: Circular reference")))'
    Game-Formula 5 'B' 'SUM(IF(VALUE(MID(U1,5,3))>0,0.1,0),IF(VALUE(MID(U1,5,3))>1,0.2,0),IF(VALUE(MID(U1,5,3))>2,PI(),0),MAX(0,VALUE(MID(U1,5,3))-3))'
    Game-Formula 5 'D' '"=SUM("&IF(VALUE(MID(U1,5,3))>0,"0.1","")&IF(VALUE(MID(U1,5,3))>1,",0.2","")&IF(VALUE(MID(U1,5,3))>2,",PI()","")&REPT(",1",MAX(0,VALUE(MID(U1,5,3))-3))&")"'
    Game-Formula 6 'B' '(LEN(U1)-11)/4'
    Game-Text 6 'D' 'Exact SUM'
    # Every possible score has an exact binary fraction with denominator 2^56. Split
    # its numerator into 8-digit pieces so Excel's 15-digit display never rounds it.
    Game-Formula 6 'F' 'INT(B5)&" + "&INT(MOD(B5,1)*2^56/100000000)&RIGHT("00000000"&MOD(MOD(B5,1)*2^56,100000000),8)&"/72057594037927936"'
    for ($row = 1; $row -le 12; $row++) {
        for ($col = 1; $col -le 16; $col++) {
            $at = $row + 6; $letter = [string][char]([int][char]'A' + $col); $pos = ($row-1)*16+$col
            $food = 'VALUE(MID($U$1,9,3))'
            $formula = 'IF(ISNUMBER(SEARCH("|' + $pos.ToString('000') + '",MID($U$1,12,999))),"o",IF(' + $pos + '=75,1/0,IF(' + $pos + '=' + $food + ',IF(VALUE(MID($U$1,5,3))=0,0.1,IF(VALUE(MID($U$1,5,3))=1,0.2,IF(VALUE(MID($U$1,5,3))=2,PI(),1))),"")))'
            Game-Formula $at $letter $formula
        }
    }
    $sheet = $Decl + '<worksheet xmlns="' + $Main + '"><cols><col min="1" max="1" width="18" customWidth="1"/><col min="2" max="17" width="9" customWidth="1"/><col min="21" max="21" hidden="1" width="9" customWidth="1"/></cols><sheetData>'
    foreach ($row in ($cells.Keys | Sort-Object)) {
        $sheet += '<row r="' + $row + '">'
        foreach ($column in ($cells[$row].Keys | Sort-Object { $_.Length }, { $_ })) { $sheet += $cells[$row][$column] }
        $sheet += '</row>'
    }
    $sheet += '</sheetData><conditionalFormatting sqref="B7:Q18"><cfRule type="expression" dxfId="0" priority="1"><formula>B7="o"</formula></cfRule><cfRule type="expression" dxfId="1" priority="2"><formula>ISNUMBER(B7)</formula></cfRule><cfRule type="expression" dxfId="2" priority="3"><formula>ISERROR(B7)</formula></cfRule></conditionalFormatting><dataValidations count="1"><dataValidation type="list" allowBlank="1" showErrorMessage="1" sqref="B2"><formula1>"L,R,U,D"</formula1></dataValidation></dataValidations></worksheet>'
    $styles = (Get-Styles).Replace('</styleSheet>', '<dxfs count="3"><dxf><fill><patternFill patternType="solid"><fgColor rgb="FF64B85A"/></patternFill></fill></dxf><dxf><fill><patternFill patternType="solid"><fgColor rgb="FFFFD25A"/></patternFill></fill></dxf><dxf><fill><patternFill patternType="solid"><fgColor rgb="FFE86D75"/></patternFill></fill></dxf></dxfs></styleSheet>')
    Write-Workbook $Path @(, @('Circular Reference', $sheet)) -IterateOnce -Styles $styles
}
