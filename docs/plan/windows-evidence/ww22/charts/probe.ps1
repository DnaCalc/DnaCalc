param($OwnedExcel,[string]$Directory)
. "$PSScriptRoot/../helpers.ps1"
$excel.DisplayAlerts=$false
function Frame($co) {
    $geometry=[ordered]@{}
    foreach($field in 'Left','Top','Width','Height') {
        $value=[double]$co.$field
        $geometry[$field]=@{value=$value;bits=('{0:x16}' -f [BitConverter]::DoubleToInt64Bits($value))}
    }
    @{objectName=$co.Name;chartName=$co.Chart.Name;geometry=$geometry}
}
$b=Book
try {
    $s=$b.Worksheets.Item(1); $s.Name='Data'
    $s.Range('A1').Value2='Alpha'; $s.Range('A2').Value2='Beta'; $s.Range('A3').Value2='Gamma'
    $s.Range('B1').Value2=3; $s.Range('B2').Value2=6; $s.Range('B3').Value2=9
    $s.Columns.Item('A:A').ColumnWidth=20
    $co=$s.ChartObjects().Add(15,20,300,200); $co.Name='First'
    $co.Chart.ChartType=51
    $ser=$co.Chart.SeriesCollection().NewSeries(); $ser.XValues=$s.Range('A1:A3'); $ser.Values=$s.Range('B1:B3')
    $co2=$s.ChartObjects().Add(330,20,120,100); $co2.Name='Second'
    $shape=$s.Shapes.AddShape(1,10,240,50,40); $shape.Name='Other'
    $b.SaveAs("$Directory/base.xlsx",51)
    foreach($index in 0,1,3,4,'Beta') { Observe "Points.$index" { $p=$ser.Points($index); @{name=$p.Name;has=$p.HasDataLabel} } }
    Observe 'Points.Collection' { $pts=$ser.Points(); @{count=$pts.Count;first=$pts.Item(1).Name} }
    Observe 'Point.Initial.Label' { $ser.Points(1).DataLabel.Text }
    Observe 'Point.Enable.One' { $ser.Points(1).HasDataLabel=$true; @{point=$ser.Points(1).HasDataLabel; text=$ser.Points(1).DataLabel.Text; series=$ser.HasDataLabels;count=$ser.DataLabels().Count} }
    $b.SaveCopyAs("$Directory/point-one.xlsx")
    $ser.HasDataLabels=$true
    Observe 'Point.Disable.Middle' {
        $ser.Points(2).HasDataLabel=$false
        $states=foreach($i in 1..3) { @{point=$i;has=$ser.Points($i).HasDataLabel} }
        @{series=$ser.HasDataLabels;count=$ser.DataLabels().Count;points=@($states)}
    }
    $b.SaveCopyAs("$Directory/point-middle-off.xlsx")
    $ser.HasDataLabels=$true
    $labels=$ser.DataLabels()
    Observe 'DataLabels.Font' {
        $before=@{size=$labels.Font.Size;bold=$labels.Font.Bold;color=$labels.Font.Color}
        $labels.Font.Size=14; $labels.Font.Bold=$true; $labels.Font.Color=255
        $after=foreach($i in 1..3) { $f=$ser.Points($i).DataLabel.Font; @{size=$f.Size;bold=$f.Bold;color=$f.Color} }
        @{before=$before;after=@($after)}
    }
    $dl=$ser.Points(1).DataLabel
    Observe 'DataLabel.NumberFormat' {
        $before=@{format=$dl.NumberFormat;linked=$dl.NumberFormatLinked;text=$dl.Text}
        $dl.NumberFormat='0.00'
        $put=@{format=$dl.NumberFormat;linked=$dl.NumberFormatLinked;text=$dl.Text}
        $dl.NumberFormatLinked=$true
        @{before=$before;put=$put;relink=@{format=$dl.NumberFormat;linked=$dl.NumberFormatLinked;text=$dl.Text};neighbor=$ser.Points(3).DataLabel.NumberFormat}
    }
    Observe 'DataLabel.TextAndPosition' {
        $before=@{text=$dl.Text;caption=$dl.Caption;left=$dl.Left;top=$dl.Top}
        $dl.Text='Custom'; $dl.Left=30.5; $dl.Top=40.25
        $s.Range('B1').Value2=12
        @{before=$before;after=@{text=$dl.Text;caption=$dl.Caption;left=$dl.Left;top=$dl.Top};neighbor=$ser.Points(3).DataLabel.Text}
    }
    $b.SaveCopyAs("$Directory/label-format.xlsx")
} finally {$b.Close($false)}
foreach($case in 'objectName','chartName','duplicate-chart','duplicate-shape','left','top','width','height') {
    $b=$excel.Workbooks.Open("$Directory/base.xlsx")
    try {
        $s=$b.Worksheets.Item(1); $co=$s.ChartObjects().Item('First')
        $path="$Directory/frame-$case.xlsx"
        Observe "Frame.$case" {
            $before=Frame $co
            switch($case) {
                'objectName' {$co.Name='New & <First>'}
                'chartName' {$co.Chart.Name='New & <First>'}
                'duplicate-chart' {$co.Name='Second'}
                'duplicate-shape' {$co.Name='Other'}
                'left' {$co.Left=50.7}
                'top' {$co.Top=50.7}
                'width' {$co.Width=150.7}
                'height' {$co.Height=150.7}
            }
            $after=Frame $co
            $inventory=for($i=1;$i -le $s.Shapes.Count;$i++) { $item=$s.Shapes.Item($i); @{name=$item.Name;id=$item.ID} }
            $b.SaveAs($path,51)
            $b.Close($false)
            $opened=$excel.Workbooks.Open($path)
            try {
                $objects=$opened.Worksheets.Item(1).ChartObjects()
                $lookups=foreach($name in 'First',$after.objectName) {
                    try {@{key=$name;found=$objects.Item($name).Name}} catch {@{key=$name;error=$_.Exception.Message}}
                }
                @{before=$before;after=$after;reopened=Frame $objects.Item(1);shapes=@($inventory);lookups=@($lookups)}
            } finally {$opened.Close($false)}
        }
    } finally {try {$b.Close($false)} catch {}}
}
