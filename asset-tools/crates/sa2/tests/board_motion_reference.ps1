param([Parameter(Mandatory)][string]$Tools, [string]$RelFile, [string]$PcDll)
$ErrorActionPreference='Stop'
$assembly=[Reflection.Assembly]::LoadFrom((Join-Path (Resolve-Path $Tools) 'tools/lib/SAModel.dll'))
if (!!$RelFile -eq !!$PcDll){throw "Supply exactly one of -RelFile or -PcDll"}
$src=[IO.File]::ReadAllBytes((Resolve-Path $(if($PcDll){$PcDll}else{$RelFile})))
if($RelFile -and [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($src)) -ne 'bcb2f3e2dbead7bafebcda84063fba54bac831cbd9102fd0230d78a5b88b570b'){throw 'Unexpected REL revision'}
# Make section-local pointer offsets available to SAModel; no game address space is needed.
Add-Type @"
using System;
public class BoardDataImage {
    public static int W(byte[] b,int n) {return (b[n]<<24)|(b[n+1]<<16)|(b[n+2]<<8)|b[n+3];}
    public static byte[] Section(byte[] b) {
        int sectionTable=W(b,16), start=W(b,sectionTable+5*8)&~1, size=W(b,sectionTable+5*8+4);
        var data=new byte[size];Array.Copy(b,start,data,0,size);
        int imports=W(b,40), end=imports+W(b,44);
        for(int i=imports;i<end;i+=8) {
            if(W(b,i)!=12)continue;
            int offset=0, section=-1;
            for(int p=W(b,i+4);p+8<=b.Length;p+=8) {
                int kind=b[p+2], target=b[p+3];
                if(kind==203)break;
                if(kind==202){section=target;offset=0;continue;}
                offset+=(b[p]<<8)|b[p+1];
                if(section==5 && kind==1 && target==5)Array.Copy(b,p+4,data,offset,4);
            }
        }
        return data;
    }
}
"@
$base=0
if($PcDll){
    if([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($src)) -ne '6ee749548ea124d22d9cbfb6f3323f6d6a9a330d9838dd2f5656be2d9b87ff59'){throw 'Unexpected PC DLL revision'}
    $pe=[BitConverter]::ToInt32($src,60)
    $opt=$pe+24
    $base=[BitConverter]::ToUInt32($src,$opt+28)
    $data=[byte[]]::new([BitConverter]::ToInt32($src,$opt+56))
    $section=$opt+[BitConverter]::ToUInt16($src,$pe+20)
    for($i=0;$i -lt [BitConverter]::ToUInt16($src,$pe+6);$i++){
        $p=$section+40*$i
        [Array]::Copy($src,[BitConverter]::ToInt32($src,$p+20),$data,[BitConverter]::ToInt32($src,$p+12),[BitConverter]::ToInt32($src,$p+16))
    }
    [SAModel.ByteConverter]::BigEndian=$false
    $slots=@{}
    for($at=0x19c7258;$at -lt 0x19c7258+456;$at+=8){
        $id=[BitConverter]::ToInt16($data,$at)
        if($id -ge 145 -and $id -le 168){$slots[[int]$id]=[BitConverter]::ToUInt32($data,$at+4)-$base}
    }
}else{
$data=[BoardDataImage]::Section($src)
[SAModel.ByteConverter]::BigEndian=$true
$slots=@{}
for ($at=0x1fd2b0;$at -lt 0x1fd478;$at+=8){
    $id=[int]$data[$at]*256+$data[$at+1]
    if ($id -ge 145 -and $id -le 168) {$slots[$id]=[BoardDataImage]::W($data,$at+4)}
}
}
$stream=[IO.MemoryStream]::new()
$writer=[IO.BinaryWriter]::new($stream)
$keyTotal=0
foreach($id in ($slots.Keys|Sort-Object)){
    $off=$slots[$id]
    $motion=[SAModel.NJS_MOTION]::new($data,$off,$base,2,$null,$false,$null,'','',$false)
    $writer.Write([uint16]$id);$writer.Write([uint32]$off);$writer.Write([uint32]$motion.Frames)
    for($part=0;$part -lt 2;$part++){
        $tracks=$motion.Models[$part]
        foreach($channel in 'Position','Rotation'){
            $track=$tracks.$channel
            $writer.Write([uint32]$track.Count)
            foreach($key in ($track.Keys|Sort-Object)){
                $writer.Write([int]$key)
                foreach($axis in 'X','Y','Z'){
                    if($channel -eq 'Position'){$writer.Write([float]$track[$key].$axis)}
                    else{$writer.Write([int]$track[$key].$axis)}
                }
            }
            $keyTotal+=$track.Count
        }
    }
}
"$($slots.Count) slots, $(($slots.Values|Sort-Object -Unique).Count) distinct headers, $keyTotal key records, SHA-256 $([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($stream.ToArray())))"
